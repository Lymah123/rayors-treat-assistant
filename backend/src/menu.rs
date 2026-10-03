use serde::Deserialize;
use std::{error::Error, fs, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct Product {
    pub category: String,
    pub name: String,
    pub size: String,
    pub price: u32,
}

#[derive(Debug, Deserialize)]
pub struct Menu {
    pub business: String,
    pub order_hours: String,
    pub products: Vec<Product>,
}

impl Menu {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn by_category(&self, category: &str) -> Vec<&Product> {
        let c = category.to_lowercase();
        self.products.iter().filter(|p| p.category == c).collect()
    }

    pub fn find(&self, category: &str, size: &str) -> Option<&Product> {
        let c = category.to_lowercase();
        let s = size.to_lowercase();
        self.products
            .iter()
            .find(|p| p.category == c && p.size.to_lowercase() == s)
    }

    pub fn find_all(&self, category: &str, size: &str) -> Vec<&Product> {
        let c = category.to_lowercase();
        let s = size.to_lowercase();
        self.products
            .iter()
            .filter(|p| p.category == c && p.size.to_lowercase() == s)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        Menu::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/menu.json")).unwrap()
    }

    #[test]
    fn parfait_has_six_sizes() {
        assert_eq!(menu().by_category("parfait").len(), 6);
    }

    #[test]
    fn finds_known_price() {
        assert_eq!(menu().find("parfait", "500ml").unwrap().price, 6500);
    }

    #[test]
    fn unknown_item_is_none() {
        assert!(menu().find("cake", "8 inch").is_none());
    }

    #[test]
    fn yoghurt_330ml_is_ambiguous() {
        assert_eq!(menu().find_all("yoghurt", "330ml").len(), 2);
    }
}
