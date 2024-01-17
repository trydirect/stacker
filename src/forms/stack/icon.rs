use serde::{Deserialize, Serialize};
use crate::forms::stack::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub light: IconLight,
    pub dark: IconDark,
}
