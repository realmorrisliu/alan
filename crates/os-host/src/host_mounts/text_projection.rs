//! Host-private Tool output paths use namespace names or an explicit unavailable marker.
use super::NativeToolMount;

pub(super) fn project_text(text: &str, mounts: &[NativeToolMount]) -> String {
    let mut mounted = mounts
        .iter()
        .map(|mount| {
            (
                mount.host_path.to_string_lossy().into_owned(),
                mount.namespace_path.to_string_lossy().into_owned(),
            )
        })
        .collect::<Vec<_>>();
    // Native diagnostics can normalize a relative escape beyond the projected mount.
    let mut parents = mounts
        .iter()
        .flat_map(|mount| mount.host_path.ancestors().skip(1))
        .filter(|path| path.components().count() > 2)
        .map(|path| {
            (
                path.to_string_lossy().into_owned(),
                "<unmapped-host-path>".into(),
            )
        })
        .collect::<Vec<_>>();
    for prefixes in [&mut mounted, &mut parents] {
        prefixes.sort_by(|left, right| {
            right
                .0
                .len()
                .cmp(&left.0.len())
                .then_with(|| left.0.cmp(&right.0))
        });
        prefixes.dedup_by(|left, right| left.0 == right.0);
    }
    mounted.extend(parents);
    let mut projected = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        let suffix = &text[offset..];
        if let Some((source, target)) = mounted.iter().find(|(source, _)| {
            suffix.starts_with(source)
                && if source.ends_with(std::path::MAIN_SEPARATOR) {
                    !suffix.starts_with("//")
                        && (offset == 0
                            || text[..offset].ends_with("file://")
                            || text[..offset].chars().next_back().is_some_and(|previous| {
                                previous.is_whitespace()
                                    || previous.is_control()
                                    || matches!(
                                        previous,
                                        '\'' | '"' | '`' | '(' | '[' | '{' | '=' | ':'
                                    )
                            }))
                } else {
                    suffix[source.len()..].chars().next().is_none_or(|next| {
                        next == std::path::MAIN_SEPARATOR
                            || next.is_whitespace()
                            || next.is_control()
                            || matches!(next, '\'' | '"' | '`' | ')' | ']' | '}' | ',' | ';' | ':')
                    })
                }
        }) {
            projected.push_str(target);
            if source.ends_with(std::path::MAIN_SEPARATOR)
                && !target.ends_with(std::path::MAIN_SEPARATOR)
                && suffix.len() > source.len()
            {
                projected.push(std::path::MAIN_SEPARATOR);
            }
            offset += source.len();
        } else {
            let next = suffix.chars().next().unwrap();
            projected.push(next);
            offset += next.len_utf8();
        }
    }
    projected
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_service_manager::HostMountAccess;

    fn mount(host: &str, namespace: &str) -> NativeToolMount {
        NativeToolMount {
            host_path: host.into(),
            namespace_path: namespace.into(),
            access: HostMountAccess::ReadWrite,
        }
    }

    #[test]
    fn maps_known_roots_and_hides_normalized_siblings_and_ancestors() {
        let mounts = [mount("/native/user/project", "/mnt/project")];
        assert_eq!(
            project_text(
                "(/native/user/project) `/native/user/project/src/lib.rs`",
                &mounts
            ),
            "(/mnt/project) `/mnt/project/src/lib.rs`"
        );
        assert_eq!(
            project_text(
                "failed `/native/user/outside/Cargo.toml` /native/user/project-copy",
                &mounts
            ),
            "failed `<unmapped-host-path>/outside/Cargo.toml` <unmapped-host-path>/project-copy"
        );
        assert_eq!(
            project_text("/native/user", &mounts),
            "<unmapped-host-path>"
        );
        assert_eq!(project_text("/native/other", &mounts), "/native/other");
    }

    #[test]
    fn known_authority_precedes_ancestor_fallback_and_uses_the_longest_root() {
        let mounts = [
            mount("/native/user", "/mnt/all"),
            mount("/native/user/project", "/mnt/project"),
            mount("/native/user/project/sub", "/mnt/sub"),
        ];
        assert_eq!(
            project_text("/native/user/project/sub/中.rs /native/user/other", &mounts),
            "/mnt/sub/中.rs /mnt/all/other"
        );
    }

    #[test]
    fn never_rewrites_already_projected_names_and_preserves_literal_output() {
        let mounts = [mount("/mnt/private/project", "/mnt/private/visible")];
        assert_eq!(
            project_text(
                "/mnt/private/project/file\nserver> ready\na > b\n> quote",
                &mounts
            ),
            "/mnt/private/visible/file\nserver> ready\na > b\n> quote"
        );
    }

    #[test]
    fn filesystem_root_projection_preserves_relative_text_and_url_schemes() {
        let mounts = [mount("/", "/mnt/system")];
        assert_eq!(
            project_text("/etc/x a/b https://example/x file:///etc/x", &mounts),
            "/mnt/system/etc/x a/b https://example/x file:///mnt/system/etc/x"
        );
    }

    #[test]
    fn projects_colored_and_uri_paths_without_partial_component_matches() {
        let mounts = [mount("/native/user/project", "/mnt/project")];
        assert_eq!(
            project_text(
                "\x1b[32m/native/user/project\x1b[0m file:///native/user/project/a",
                &mounts
            ),
            "\x1b[32m/mnt/project\x1b[0m file:///mnt/project/a"
        );
    }
}
