//! Utilities for case-insensitive enum serialization/deserialization
//!
//! This module provides macros and utilities to help with case-insensitive
//! handling of enum variants in JSON and other serialization formats.

/// Macro to implement case-insensitive deserialization for enums with simple string variants.
///
/// This macro generates a custom `Deserialize` implementation for an enum that makes
/// variant string matching case-insensitive. It works with enums that use string
/// representation in serialization (e.g., with `#[serde(tag = "type")]`).
///
/// The generated implementation will match variant names case-insensitively, so both
/// `"variant1"` and `"VARIANT1"` will be deserialized as `MyEnum::Variant1`.
#[macro_export]
macro_rules! impl_case_insensitive_enum {
    ($enum_name:ident, { $($variant_str:expr => $variant:ident),* $(,)? }) => {
        impl<'de> ::serde::Deserialize<'de> for $enum_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                use ::serde::de::{self, MapAccess, Visitor};
                use std::fmt;

                struct EnumVisitor;

                impl<'de> Visitor<'de> for EnumVisitor {
                    type Value = $enum_name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str(concat!("a struct with a `type` field for ", stringify!($enum_name)))
                    }

                    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
                    where
                        M: MapAccess<'de>,
                    {
                        let mut type_: Option<String> = None;
                        let mut value: Option<::serde_json::Value> = None;

                        while let Some(key) = map.next_key::<String>()? {
                            if key == "type" {
                                type_ = Some(map.next_value()?);
                            } else if key == "value" {
                                value = Some(map.next_value()?);
                            } else {
                                let _: ::serde_json::Value = map.next_value()?;
                            }
                        }

                        let type_ = type_.ok_or_else(|| de::Error::missing_field("type"))?;
                        let value = value.ok_or_else(|| de::Error::missing_field("value"))?;
                        let type_lowercase = type_.to_lowercase();

                        match type_lowercase.as_str() {
                            $(
                                $variant_str => {
                                    let content = ::serde_json::from_value::<String>(value)
                                        .map_err(|e| de::Error::custom(format!(
                                            concat!("invalid ", $variant_str, " value: {}"), e
                                        )))?;
                                    Ok($enum_name::$variant(content.into()))
                                },
                            )*
                            _ => Err(de::Error::unknown_variant(
                                &type_,
                                &[$($variant_str),*],
                            )),
                        }
                    }
                }

                deserializer.deserialize_map(EnumVisitor)
            }
        }
    };
}

/// Macro to implement case-insensitive deserialize for struct enum variants
///
/// Similar to `impl_case_insensitive_enum` but for enums where variants contain structs
/// rather than simple types.
#[macro_export]
macro_rules! impl_case_insensitive_enum_struct {
    ($enum_name:ident, { $($variant_str:expr => $variant:ident),* $(,)? }) => {
        impl<'de> ::serde::Deserialize<'de> for $enum_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                use ::serde::de::{self, MapAccess, Visitor};
                use std::fmt;

                #[derive(::serde::Deserialize)]
                struct TypeField {
                    #[serde(rename = "type")]
                    type_: String,
                    #[serde(flatten)]
                    rest: ::serde_json::Value,
                }

                struct EnumVisitor;

                impl<'de> Visitor<'de> for EnumVisitor {
                    type Value = $enum_name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str(concat!("a struct with a `type` field for ", stringify!($enum_name)))
                    }

                    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
                    where
                        M: MapAccess<'de>,
                    {
                        // First deserialize into an intermediate structure to extract the type field
                        let TypeField { type_, rest } = ::serde::Deserialize::deserialize(
                            ::serde::de::value::MapAccessDeserializer::new(map)
                        )?;

                        let type_lowercase = type_.to_lowercase();

                        // Then match on the type field to determine the variant
                        match type_lowercase.as_str() {
                            $(
                                $variant_str => {
                                    // Create a new value with the corrected type field
                                    let mut json_value = rest;
                                    if let ::serde_json::Value::Object(ref mut map) = json_value {
                                        map.insert("type".to_string(), ::serde_json::Value::String(stringify!($variant).to_string()));
                                    }

                                    // Deserialize into the enum
                                    ::serde_json::from_value(json_value)
                                        .map_err(de::Error::custom)
                                }
                            )*
                            _ => Err(de::Error::unknown_variant(
                                &type_,
                                &[$($variant_str),*],
                            )),
                        }
                    }
                }

                deserializer.deserialize_map(EnumVisitor)
            }
        }
    };
}

#[cfg(test)]
mod tests {
	use serde::Serialize;

	#[test]
	fn test_impl_case_insensitive_enum() {
		#[derive(Debug, Clone, Serialize, PartialEq)]
		#[serde(tag = "type", content = "value")]
		enum MyEnum {
			Variant1(String),
			Variant2(String),
		}

		impl_case_insensitive_enum!(MyEnum, {
			"variant1" => Variant1,
			"variant2" => Variant2,
		});

		let json = r#"{"type": "variant1", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant1("test".to_string()));

		let json = r#"{"type": "VARIANT1", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant1("test".to_string()));

		let json = r#"{"type": "Variant1", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant1("test".to_string()));

		let json = r#"{"type": "variant2", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant2("test".to_string()));

		let json = r#"{"type": "VARIANT2", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant2("test".to_string()));

		let json = r#"{"type": "Variant2", "value": "test"}"#;
		let deserialized: MyEnum = serde_json::from_str(json).unwrap();
		assert_eq!(deserialized, MyEnum::Variant2("test".to_string()));

		let json = r#"{"type": "variant3", "value": "test"}"#;
		let deserialized: Result<MyEnum, serde_json::Error> = serde_json::from_str(json);
		assert!(deserialized.is_err());

		let json = r#"{"type": "VARIANT3", "value": "test"}"#;
		let deserialized: Result<MyEnum, serde_json::Error> = serde_json::from_str(json);
		assert!(deserialized.is_err());

		let json = r#"{"type": "Variant3", "value": "test"}"#;
		let deserialized: Result<MyEnum, serde_json::Error> = serde_json::from_str(json);
		assert!(deserialized.is_err());
	}
}
