mod oracle;
mod priceoracle;

pub use oracle::*;

// Exposing original priceoracle DTO allows to decrease
// gas consumption from 25 to 19 TGas (~24%).
pub use priceoracle::{AssetOptionalPrice, Price, PriceData};
