use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes;
use std::str::FromStr;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default, Ord, PartialOrd)]
#[serde(try_from = "String")]
pub struct Extension(String);

impl FromStr for Extension {
    type Err = dctypes::ExtensionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let owned = s.to_owned();
        Extension::try_from(owned)
    }
}

impl TryFrom<String> for Extension {
    type Error = dctypes::ExtensionParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.starts_with("x-") {
            Ok(Self(s))
        } else {
            Err(dctypes::ExtensionParseError(s))
        }
    }
}
