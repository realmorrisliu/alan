"""Offline fixture integrity checks, never real-model qualification results."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest

import repeated_development_fixtures as fixtures


class DevelopmentFixturesTest(unittest.TestCase):
    def test_complete_matrix_and_protected_verifiers(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "prepared"
            manifest = fixtures.prepare(output)
            self.assertEqual(len(manifest["slots"]), 30)
            self.assertEqual(manifest["qualified_completions"], 0)
            self.assertFalse(manifest["runtime_readiness_frozen"])
            for row in manifest["slots"]:
                project = Path(row["project"])
                verifier = Path(row["verifier"])
                self.assertFalse(verifier.is_relative_to(project))
                self.assertEqual(fixtures.inventory(project), row["baseline"])
                self.assertEqual(fixtures.digest(verifier.read_bytes()), row["verifier_sha256"])
                self.assertEqual(fixtures.inventory(verifier.parent), row["protected_baseline"])
                self.assertEqual(set(row["protected_baseline"]), {"verify.rs"})
                scratch = Path(row["checker_scratch"])
                self.assertFalse(scratch.is_relative_to(project))
                self.assertEqual(fixtures.inventory(scratch), row["checker_baseline"])
                self.assertEqual(set(row["checker_baseline"]), {"Cargo.toml", "Cargo.lock"})
                driver = tomllib.loads((scratch / "Cargo.toml").read_text())
                self.assertEqual(driver["lib"]["path"], str(verifier))
                self.assertEqual(driver["dependencies"]["qualification"]["path"], str(project))
                self.assertEqual(fixtures.digest(row["prompt"].encode()), row["prompt_sha256"])
                self.assertEqual(row["first_outcome"], "NOT_RUN")
                if row["family"] == 5 and row["repeat"] == 3:
                    self.assertGreater(row["initial_git_diff"]["initial_diff_bytes"], 100000)
                    self.assertEqual(len(row["initial_git_diff"]["baseline_commit"]), 40)
            original = (output / "manifest.json").read_bytes()
            with self.assertRaises(FileExistsError):
                fixtures.prepare(output)
            self.assertEqual((output / "manifest.json").read_bytes(), original)

    def test_missing_duplicate_and_unexpected_slots_rejected(self):
        rows = [{"slot": f"{host}-f{family}-r{repeat}"}
                for host in ("macos", "linux") for family in range(1, 6)
                for repeat in range(1, 4)]
        fixtures.validate_slots(rows)
        for invalid in (rows[:-1], rows + [rows[0]], rows[:-1] + [{"slot": "other"}]):
            with self.assertRaisesRegex(ValueError, "matrix slots"):
                fixtures.validate_slots(invalid)

    def test_scope_detects_added_deleted_and_immutable_changes(self):
        baseline = {"src/lib.rs": "a", "tests/contract.rs": "b", "README.md": "c"}
        final = {"src/lib.rs": "fixed", "tests/contract.rs": "weakened", "extra.rs": "d"}
        self.assertEqual(fixtures.changed_files(baseline, final),
                         ["README.md", "extra.rs", "src/lib.rs", "tests/contract.rs"])

    def test_inventory_rejects_file_and_directory_symlinks(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "project"
            root.mkdir()
            outside = Path(temp) / "outside"
            outside.mkdir()
            (outside / "secret").write_text("not a fixture")
            link = root / "link"
            for target in (outside, outside / "secret"):
                link.symlink_to(target)
                with self.assertRaisesRegex(ValueError, "symlink"):
                    fixtures.inventory(root)
                link.unlink()

    def test_lifecycle_and_long_output_require_extra_evidence(self):
        for repeat in range(1, 4):
            files, allowed, prompt, followups, verifier = fixtures.fixture(4, repeat, "slot")
            self.assertIn("Duration::from_secs(40)", files["tests/lifecycle.rs"])
            self.assertNotIn("effects/late.txt", allowed)
            self.assertEqual(len(followups), 2)
            files, allowed, prompt, followups, verifier = fixtures.fixture(5, repeat, "slot")
            self.assertEqual(allowed, ["src/lib.rs"])
            value = str(42000 + repeat * 137)
            self.assertIn(value, verifier)
            if repeat < 3:
                self.assertIn("line == 2807", files["tests/output.rs"])
            else:
                self.assertEqual(files["data/long.txt"].splitlines()[2806], "QVALUE=" + value)

    @unittest.skipUnless(shutil.which("rustc"), "native Rust compiler unavailable")
    def test_seeded_failures_and_protected_corrected_behavior(self):
        # These authored self-check solutions are never mounted or counted as model outcomes.
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "prepared"
            manifest = fixtures.prepare(output)
            for row in manifest["slots"]:
                if row["platform"] != "macos":
                    continue
                with self.subTest(slot=row["slot"]):
                    root = Path(row["project"])
                    protected = Path(row["verifier"]).parent
                    library = protected / "libqualification.rlib"
                    binary = protected / "verify"
                    def run(*args):
                        return subprocess.run(args, cwd=root, capture_output=True,
                                              text=True, timeout=30)
                    def compile_library():
                        return run("rustc", "--edition=2024", "--crate-name", "qualification",
                                   "--crate-type", "rlib", "src/lib.rs", "-o", str(library))
                    def verify_behavior():
                        result = run("rustc", "--edition=2024", "--test", row["verifier"],
                                     "--extern", "qualification=" + str(library), "-o", str(binary))
                        return result if result.returncode else run(str(binary))
                    family, repeat = row["family"], row["repeat"]
                    initial = compile_library()
                    if family == 3:
                        self.assertNotEqual(initial.returncode, 0)
                        self.assertIn("error", initial.stderr)
                    else:
                        self.assertEqual(initial.returncode, 0, initial.stderr)
                        self.assertNotEqual(verify_behavior().returncode, 0)
                    if family == 1:
                        expression = ["x + y", "(x - y).abs()", "x.clamp(-5, 5)"][repeat - 1]
                        fixed = f"pub fn compute(x: i32, y: i32) -> i32 {{ {expression} }}\n"
                    elif family == 2:
                        expression = ["value + adjustment", "value * adjustment",
                                      "value.min(adjustment)"][repeat - 1]
                        fixed = (f"pub fn calculate_{repeat}(value: i32, adjustment: i32) -> i32 "
                                 f"{{ {expression} }}\n")
                        for name, args in [("src/main.rs", "17, 3"), ("src/bin/report.rs", "-5, 4")]:
                            (root / name).write_text(
                                f'fn main() {{ println!("{{}}", qualification::calculate_{repeat}({args})); }}\n')
                    elif family == 3:
                        fixed = (root / "src/lib.rs").read_text()
                        if repeat == 1:
                            fixed = "use std::collections::BTreeSet;\n" + fixed
                        elif repeat == 2:
                            fixed = fixed.replace("operation", "operations")
                        else:
                            fixed = fixed.replace('value.parse::<i32>()', 'value.parse::<i32>().unwrap_or(0)')
                    elif family == 4:
                        fixed = "pub fn compute(x: i32, y: i32) -> i32 { x + y }\n"
                    else:
                        fixed = f"pub fn observed() -> i32 {{ {42000 + repeat * 137} }}\n"
                    (root / "src/lib.rs").write_text(fixed)
                    compiled = compile_library()
                    self.assertEqual(compiled.returncode, 0, compiled.stderr)
                    verified = verify_behavior()
                    self.assertEqual(verified.returncode, 0, verified.stderr + verified.stdout)
                    changes = fixtures.changed_files(row["baseline"], fixtures.inventory(root))
                    self.assertEqual(changes, sorted(row["allowed_changes"][:3] if family == 2
                                                     else ["src/lib.rs"]))
                    if family == 2:
                        expected = [(20, -1), (51, -20), (3, -5)][repeat - 1]
                        for name, value in zip(["src/main.rs", "src/bin/report.rs"], expected):
                            compiled = run("rustc", "--edition=2024", name, "--extern",
                                           "qualification=" + str(library), "-o", str(binary))
                            self.assertEqual(compiled.returncode, 0, compiled.stderr)
                            self.assertEqual(run(str(binary)).stdout.strip(), str(value))


if __name__ == "__main__":
    unittest.main()
