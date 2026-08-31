use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// Reject duplicate map keys instead of letting deserialization silently replace authority records.
pub(super) fn unique_map<'de, D, T, const LIMIT: usize>(
    de: D,
) -> Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedMap<T, const LIMIT: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const LIMIT: usize> Visitor<'de> for BoundedMap<T, LIMIT> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded map with unique keys")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut input: M) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some(key) = input.next_key::<String>()? {
                if key.len() > 128 || values.len() >= LIMIT || values.contains_key(&key) {
                    return Err(M::Error::custom("Invalid collaboration snapshot map"));
                }
                values.insert(key, input.next_value()?);
            }
            Ok(values)
        }
    }
    de.deserialize_map(BoundedMap::<T, LIMIT>(PhantomData))
}
