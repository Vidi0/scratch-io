use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::de::{Visitor, SeqAccess, Error, MapAccess, IgnoredAny};
use std::fmt;
use std::marker::PhantomData;
use std::collections::{BTreeMap, HashMap};

pub fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error> where
  D: serde::de::Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(PhantomData<T>);

  impl<'de, T> Visitor<'de> for Helper<T> where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str("an array or an empty object")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error> where
      A: SeqAccess<'de>,
    {
      let mut items = Vec::new();
      while let Some(item) = seq.next_element()? {
        items.push(item);
      }
      Ok(items)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Vec<T>, A::Error> where
      A: MapAccess<'de>,
    {
      // Consume all keys without using them, returning empty Vec
      while let Some((_k, _v)) = map.next_entry::<IgnoredAny, IgnoredAny>()? {
        // Just ignore
      }
      Ok(vec![])
    }
  }

  deserializer.deserialize_any(Helper(PhantomData))
}

/// Serialize HashMap<u64, V> as a map with string keys.
pub fn serialize_u64_map<S, V>(map: &HashMap<u64, V>, serializer: S,) -> Result<S::Ok, S::Error> where
  S: Serializer,
  V: Serialize,
{
  // Use an ordered map for deterministic output; use HashMap if you prefer.
  let mut tmp: BTreeMap<String, &V> = BTreeMap::new();
  for (k, v) in map {
    tmp.insert(k.to_string(), v);
  }
  tmp.serialize(serializer)
}

/// Deserialize a map with string keys into HashMap<u64, V>.
pub fn deserialize_u64_map<'de, D, V>(deserializer: D) -> Result<HashMap<u64, V>, D::Error> where
  D: Deserializer<'de>,
  V: Deserialize<'de>,
{
  let tmp: BTreeMap<String, V> = BTreeMap::deserialize(deserializer)?;
  let mut map = HashMap::with_capacity(tmp.len());
  for (k, v) in tmp {
    let num = k.parse::<u64>().map_err(|e| Error::custom(format!("invalid u64 key `{}`: {}", k, e)))?;
    map.insert(num, v);
  }
  Ok(map)
}
