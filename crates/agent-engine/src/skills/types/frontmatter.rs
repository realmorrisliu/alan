use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;

use super::SkillCompatibility;

/// Deserialize YAML `null` or missing values using `T::default()`.
pub(super) fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Deserialize frontmatter `tags` from a YAML sequence, comma-separated string, or `null`.
pub(super) fn deserialize_tolerant_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a sequence of tag strings or a comma-separated tag string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut tags = Vec::new();
            while let Some(tag) = seq.next_element::<String>()? {
                tags.push(tag);
            }
            Ok(tags)
        }
    }

    deserializer.deserialize_any(TagsVisitor)
}

/// Deserialize `compatibility` from either Alan's structured mapping, an `agentskills.io` summary
/// string (mapped to `requirements`), or `null`, without weakening typed `dependencies`.
pub(super) fn deserialize_tolerant_skill_compatibility<'de, D>(
    deserializer: D,
) -> Result<SkillCompatibility, D::Error>
where
    D: Deserializer<'de>,
{
    struct CompatibilityVisitor;

    impl<'de> Visitor<'de> for CompatibilityVisitor {
        type Value = SkillCompatibility;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a compatibility mapping, compatibility string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(SkillCompatibility::default())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(SkillCompatibility::default())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let trimmed = value.trim();
            Ok(SkillCompatibility {
                requirements: (!trimmed.is_empty()).then(|| trimmed.to_string()),
                ..SkillCompatibility::default()
            })
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            SkillCompatibility::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(CompatibilityVisitor)
}
