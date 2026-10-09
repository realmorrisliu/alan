#!/usr/bin/env python3
"""Coordinate repository-owned Cargo output without claiming existing caches."""

import argparse
from contextlib import ExitStack, contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time

CACHE_TAG = b"Signature: 8a477f597d28d172789f06886806bc55\n"


def git(owner, *args):
    return subprocess.check_output(["git", "-C", str(owner), *args], text=True).strip()


def owner_root(path):
    path = Path(path).resolve(strict=True)
    if Path(git(path, "rev-parse", "--show-toplevel")).resolve() != path:
        raise ValueError("build owner must be a checkout root")
    return path


def registry(owner):
    return Path(git(owner, "rev-parse", "--absolute-git-dir")) / "alan-build-artifacts"


def identity(path):
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError(f"not an owned directory: {path}")
    if path.resolve() != path:
        raise ValueError(f"output path changed or traverses a symlink: {path}")
    return [info.st_dev, info.st_ino]


def sidecar(path, suffix):
    return path.parent / f".{path.name}.alan-build.{suffix}"


@contextmanager
def locked(path, exclusive=True, create=True, timeout=0):
    flags = os.O_RDWR | os.O_NOFOLLOW | (os.O_CREAT if create else 0)
    fd = os.open(path, flags, 0o600)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1:
            raise ValueError(f"unsafe build lock: {path}")
        operation = fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH
        deadline = time.monotonic() + timeout
        while True:
            try:
                fcntl.flock(fd, operation | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(0.01)
        yield fd
    finally:
        os.close(fd)


def read_receipt(path):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW)) as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1:
            raise ValueError(f"unsafe build receipt: {path}")
        return json.load(source)


def write_receipt(path, receipt):
    # Replace within the parent filesystem; callers hold its output lock.
    temporary = path.with_name(path.name + f".{os.getpid()}.tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, "w") as output:
            json.dump(receipt, output, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def output_roots(workspace, target=None):
    env = dict(os.environ)
    if target is not None:
        env["CARGO_TARGET_DIR"] = str(target)
    result = subprocess.check_output([
        "cargo", "metadata", "--manifest-path", str(workspace / "Cargo.toml"),
        "--no-deps", "--locked", "--offline", "--format-version", "1",
    ], cwd=workspace, env=env, text=True)
    metadata = json.loads(result)
    target = Path(metadata["target_directory"]).absolute()
    build = Path(metadata.get("build_directory", target)).absolute()
    return sorted({target, build})


def validate_location(owner, path):
    if path.is_symlink() or path.resolve() != path:
        raise ValueError(f"output must not traverse symlinks: {path}")
    checkouts = [Path(line[9:]).resolve() for line in
                 git(owner, "worktree", "list", "--porcelain").splitlines()
                 if line.startswith("worktree ")]
    data_home = Path(os.environ.get("XDG_DATA_HOME", ""))
    if not data_home.is_absolute():
        data_home = Path.home() / ".local/share"
    private = [Path(git(owner, "rev-parse", "--path-format=absolute", "--git-common-dir")),
               Path.home() / "Library/Application Support/Alan",
               Path.home() / ".local/share/Alan",
               data_home / "Alan",
               Path.home() / ".alan", Path.home() / ".alan-dev"]
    private = [path.resolve() for path in private]
    protected = checkouts + private
    if any(p == path or path in p.parents for p in protected):
        raise ValueError(f"output contains source or Git state: {path}")
    if any(p == path or p in path.parents for p in protected[len(checkouts):]):
        raise ValueError(f"output is inside Git state: {path}")


def register_output(owner, path, purpose):
    validate_location(owner, path)
    receipt_path = sidecar(path, "json")
    expected = {"version": 1, "owner": str(owner), "path": str(path), "purpose": purpose}
    if receipt_path.exists() or receipt_path.is_symlink():
        receipt = read_receipt(receipt_path)
        if any(receipt.get(key) != value for key, value in expected.items()):
            raise ValueError(f"build output has another owner or purpose: {path}")
        if path.exists() and receipt.get("identity") != identity(path):
            raise ValueError(f"build output was replaced: {path}")
    elif path.exists() and any(path.iterdir()):
        if owner / "target" in [path, *path.parents] and purpose != "task":
            print(f"Unmanaged existing output; cleanup will skip: {path}", file=sys.stderr)
            return
        raise ValueError(f"refusing to claim nonempty unowned output: {path}")
    path.mkdir(parents=True, exist_ok=True)
    if not any(path.iterdir()):
        # Cargo writes this only when it creates the root itself.
        with (path / "CACHEDIR.TAG").open("xb") as tag:
            tag.write(CACHE_TAG)
    receipt = expected | {"identity": identity(path)}
    write_receipt(receipt_path, receipt)
    records = registry(owner)
    records.mkdir(mode=0o700, parents=True, exist_ok=True)
    key = hashlib.sha256(str(path).encode()).hexdigest()
    write_receipt(records / f"{key}.json", receipt)


def admission_lock(owner):
    common = Path(git(owner, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    return common / "alan-build-admission.lock"


def lease_ancestors(path, stack, descriptors):
    for ancestor in reversed(path.parents):
        lock = sidecar(ancestor, "lock")
        if str(ancestor) not in descriptors and (lock.exists() or lock.is_symlink()):
            descriptors[str(ancestor)] = stack.enter_context(
                locked(lock, exclusive=False, create=False))


@contextmanager
def build_lease(owner, roots, purpose):
    descriptors = inherited_leases(owner)
    with ExitStack() as stack:
        # Serialize topology changes, not compilation, across linked worktrees.
        with locked(admission_lock(owner), timeout=30):
            for path in sorted(set(roots)):
                validate_location(owner, path)
                lease_ancestors(path, stack, descriptors)
                if str(path) in descriptors:
                    continue
                path.parent.mkdir(parents=True, exist_ok=True)
                try:
                    fd = stack.enter_context(locked(sidecar(path, "lock")))
                except BlockingIOError:
                    fd = stack.enter_context(locked(sidecar(path, "lock"), exclusive=False))
                    record = registered_outputs(owner).get(path)
                    if record is None or record[1]["purpose"] != purpose:
                        raise ValueError(f"busy output has no matching ownership receipt: {path}")
                    verify_receipt(owner, path, record[1])
                else:
                    register_output(owner, path, purpose)
                descriptors[str(path)] = fd
            records = registered_outputs(owner)
            for path in roots:
                # A different repository may have registered an ancestor meanwhile.
                lease_ancestors(path, stack, descriptors)
                if path in records:
                    verify_receipt(owner, path, records[path][1])
                elif (owner / "target" in [path, *path.parents]
                      and not sidecar(path, "json").exists()
                      and not sidecar(path, "json").is_symlink()):
                    identity(path)
                else:
                    raise ValueError(f"inherited output has no ownership receipt: {path}")
            for fd in descriptors.values():
                fcntl.flock(fd, fcntl.LOCK_SH)
        yield descriptors


def inherited_leases(owner):
    value = json.loads(os.environ.get("ALAN_BUILD_LEASE", "{}"))
    if not value:
        return {}
    if value.get("owner") != str(owner):
        raise ValueError("inherited build lease belongs to another checkout")
    locks = value["locks"]
    for path, fd in locks.items():
        opened = os.fstat(fd)
        current = sidecar(Path(path), "lock").lstat()
        if (opened.st_dev, opened.st_ino) != (current.st_dev, current.st_ino):
            raise ValueError(f"inherited build lock was replaced: {path}")
        fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
    return locks


def registered_outputs(owner):
    records = registry(owner)
    if not records.exists():
        return {}
    result = {}
    for path in sorted(records.glob("*.json")):
        receipt = read_receipt(path)
        if receipt.get("version") != 1 or receipt.get("owner") != str(owner):
            raise ValueError(f"invalid checkout output receipt: {path}")
        output = Path(receipt["path"])
        validate_location(owner, output)
        if not output.is_absolute():
            raise ValueError(f"relative output in receipt: {path}")
        result[output] = (path, receipt)
    return result


def open_paths():
    result = subprocess.run(["lsof", "-nP", "-a", "-u", str(os.getuid()), "-p", f"^{os.getpid()}", "-F", "n"],
                            capture_output=True, text=True)
    if result.returncode or result.stderr.strip():
        raise ValueError("cannot establish process/open-file state; cleanup skipped")
    return [Path(line[1:]).resolve() for line in result.stdout.splitlines()
            if line.startswith("n/")]


def in_use(root, paths):
    return any(path == root or root in path.parents for path in paths)


def verify_receipt(owner, root, record):
    validate_location(owner, root)
    if read_receipt(sidecar(root, "json")) != record:
        raise ValueError(f"ownership receipt changed: {root}")
    if record["identity"] != identity(root):
        raise ValueError(f"output was replaced: {root}")


def reject_nested_ownership(root):
    # Sidecars cover linked worktrees, other repositories and orphaned receipts alike.
    with os.scandir(root) as entries:
        for entry in entries:
            if entry.name.startswith(".") and entry.name.endswith(".alan-build.json"):
                raise ValueError(f"contains separately owned output: {entry.path}")
            if entry.is_dir(follow_symlinks=False):
                reject_nested_ownership(entry.path)


def cargo_only(root):
    # The outer target often also contains harness evidence; never erase that.
    known = {"debug", "release", "doc", "package", "tmp", ".rustc_info.json", "CACHEDIR.TAG"}
    known.update(subprocess.check_output(["rustc", "--print", "target-list"], text=True).splitlines())
    unknown = [path.name for path in root.iterdir() if path.name not in known]
    if unknown:
        raise ValueError(f"unclassified output entries in {root}: {', '.join(sorted(unknown))}")


def inventory(owner):
    records = registered_outputs(owner)
    roots = set(records) | set(output_roots(owner)) | {owner / "target/quality-gate"}
    rows = []
    try:
        consumers = open_paths()
    except (ValueError, OSError):
        consumers = None
    for root in sorted(roots):
        row = {"path": str(root), "owner": str(owner), "state": "unknown"}
        try:
            validate_location(owner, root)
            if not root.exists():
                row["state"] = "absent"
            else:
                identity(root)
                row["activity"] = ("unknown" if consumers is None else
                                   "active" if in_use(root, consumers) else "idle-at-scan")
                row["allocated_kib"] = int(subprocess.check_output(
                    ["du", "-sk", str(root)], text=True).split()[0])
                if root in records:
                    with locked(sidecar(root, "lock"), create=False):
                        verify_receipt(owner, root, records[root][1])
                        row.update(state="managed", purpose=records[root][1]["purpose"])
        except BlockingIOError:
            row["state"] = "active"
        except (ValueError, OSError, subprocess.CalledProcessError) as error:
            row.update(state="unsafe", reason=str(error))
        rows.append(row)
    return rows


def clean_output(owner, root, record_path, record, apply=False):
    with ExitStack() as locks:
        admission = locks.enter_context(locked(admission_lock(owner)))
        output_lock = locks.enter_context(locked(sidecar(root, "lock"), create=False))
        if not root.exists():
            return "absent"
        verify_receipt(owner, root, record)
        if read_receipt(record_path) != record:
            raise ValueError(f"checkout receipt changed: {root}")
        nested = [path for path in registered_outputs(owner) if root in path.parents]
        if nested:
            raise ValueError(f"contains separately owned output: {root}")
        reject_nested_ownership(root)
        cargo_only(root)
        with os.fdopen(os.open(root / "CACHEDIR.TAG", os.O_RDONLY | os.O_NOFOLLOW), "rb") as tag:
            if tag.read(len(CACHE_TAG) - 1) != CACHE_TAG.rstrip(b"\n"):
                raise ValueError(f"invalid Cargo cache tag: {root}")
        # Whole-directory cargo clean does not take Cargo's per-profile locks.
        native = sorted(set(root.glob("*/.cargo-lock")) | set(root.glob("*/*/.cargo-lock")))
        for path in native:
            if path.resolve() != path:
                raise ValueError(f"native Cargo lock traverses a symlink: {path}")
            locks.enter_context(locked(path, create=False))
        if in_use(root, open_paths()):
            raise ValueError(f"active process or consumer: {root}")
        if not apply:
            return "would-clean"
        verify_receipt(owner, root, record)
        # Scope both output settings so ambient build-dir cannot widen deletion.
        subprocess.run([
            "cargo", "clean", "--manifest-path", str(owner / "Cargo.toml"),
            "--target-dir", str(root), "--config", "build.build-dir=" + json.dumps(str(root)),
        ], cwd=owner, check=True, pass_fds=(admission, output_lock))
        if root.exists():
            return "retained-or-recreated"
        record_path.unlink()
        return "cleaned"


def run_command(owner, workspace, roots, purpose, command, target=None, evidence=None):
    report = None
    if purpose == "task":
        if evidence is None:
            raise ValueError("disposable tasks require --evidence-dir outside compiler output")
        evidence = evidence.absolute()
        validate_location(owner, evidence)
        if any(evidence == root or root in evidence.parents for root in roots):
            raise ValueError("task evidence must be outside compiler output")
        evidence.mkdir(parents=True, exist_ok=True)
        identity(evidence)
        if (evidence / "build.json").exists():
            raise ValueError("task evidence already exists; select a new evidence directory")
        report = {"source_sha": git(workspace, "rev-parse", "HEAD"),
                  "source_dirty": bool(git(workspace, "status", "--porcelain")),
                  "command": command, "workspace": str(workspace),
                  "outputs": list(map(str, roots)), "state": "running"}
    with ExitStack() as stack:
        log = None
        if report is not None:
            log = stack.enter_context((evidence / "build.log").open("x"))
            write_receipt(evidence / "build.json", report)
            print(f"Disposable build diagnostics: {evidence}", flush=True)
        locks = stack.enter_context(build_lease(owner, roots, purpose))
        env = dict(os.environ)
        env["ALAN_BUILD_LEASE"] = json.dumps({"owner": str(owner), "locks": locks})
        env["ALAN_BUILD_OWNER"] = str(owner)
        if target is not None:
            env["CARGO_TARGET_DIR"] = str(target)
        if purpose == "task":
            env["CARGO_INCREMENTAL"] = "0"
            env["CARGO_BUILD_BUILD_DIR"] = str(target)
        try:
            result = subprocess.call(command, cwd=workspace, env=env, pass_fds=set(locks.values()),
                                     stdout=log, stderr=subprocess.STDOUT if log else None)
        except BaseException:
            if report is not None:
                write_receipt(evidence / "build.json", report | {"state": "interrupted"})
            raise
    if report is not None:
        report.update(state="finished", exit_code=result, cleanup=[])
        write_receipt(evidence / "build.json", report)
        for root in roots:
            try:
                path, record = registered_outputs(owner)[root]
                state = clean_output(owner, root, path, record, apply=True)
                report["cleanup"].append({"path": str(root), "state": state})
            except (ValueError, OSError, subprocess.CalledProcessError) as error:
                report["cleanup"].append({"path": str(root), "state": "retained", "reason": str(error)})
        write_receipt(evidence / "build.json", report)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owner", type=Path, default=Path.cwd())
    sub = parser.add_subparsers(dest="action", required=True)
    run = sub.add_parser("run", help="hold output leases for a build or verification command")
    run.add_argument("--workspace", type=Path)
    run.add_argument("--target-dir", type=Path)
    run.add_argument("--purpose", choices=["checkout", "quality", "task"], default="checkout")
    run.add_argument("--evidence-dir", type=Path, help="retained diagnostics for disposable tasks")
    run.add_argument("command", nargs=argparse.REMAINDER)
    sub.add_parser("status", help="report output ownership and allocated sizes as JSON")
    clean = sub.add_parser("clean", help="preview cleanup of registered idle Cargo output")
    clean.add_argument("--apply", action="store_true", help="apply the previewed cleanup policy")
    args = parser.parse_args()
    owner = owner_root(args.owner)
    if args.action == "status":
        from service_scratch import scratch_outputs
        print(json.dumps({"build_outputs": inventory(owner), "service_scratch": scratch_outputs()}, indent=2))
        return 0
    if args.action == "clean":
        records = registered_outputs(owner)
        results = []
        for root, (record_path, record) in records.items():
            try:
                state = clean_output(owner, root, record_path, record, args.apply)
                results.append({"path": str(root), "state": state})
            except (ValueError, OSError, subprocess.CalledProcessError) as error:
                results.append({"path": str(root), "state": "skipped", "reason": str(error)})
        from service_scratch import scratch_outputs
        print(json.dumps({"apply": args.apply, "outputs": results,
                          "service_scratch": scratch_outputs(args.apply),
                          "unregistered_outputs": "not eligible; see status"}, indent=2))
        return 0
    workspace = (args.workspace or owner).resolve(strict=True)
    target = args.target_dir
    if target is not None:
        target = (workspace / target).absolute()
    if args.purpose == "task":
        if target is None or args.evidence_dir is None:
            parser.error("task output requires --target-dir and --evidence-dir")
        # Disposable output never borrows an ambient intermediate directory.
        os.environ["CARGO_BUILD_BUILD_DIR"] = str(target)
    roots = output_roots(workspace, target)
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("run requires a command after --")
    return run_command(owner, workspace, roots, args.purpose, command, target, args.evidence_dir)



if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
