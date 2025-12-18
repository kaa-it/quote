use rand::prelude::*;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

static SUPPORTED_TICKERS: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    HashSet::from_iter(vec![
        "AAPL", "MSFT", "GOOGL", "AMZN", "NVDA", "META", "TSLA", "JPM", "JNJ", "V", "PG", "UNH",
        "HD", "DIS", "PYPL", "NFLX", "ADBE", "CRM", "INTC", "CSCO", "PFE", "ABT", "TMO", "ABBV",
        "LLY", "PEP", "COST", "TXN", "AVGO", "ACN", "QCOM", "DHR", "MDT", "NKE", "UPS", "RTX",
        "HON", "ORCL", "LIN", "AMGN", "LOW", "SBUX", "SPG", "INTU", "ISRG", "T", "BMY", "DE",
        "PLD", "CI", "CAT", "GS", "UNP", "AMT", "AXP", "MS", "BLK", "GE", "SYK", "GILD", "MMM",
        "MO", "LMT", "FISV", "ADI", "BKNG", "C", "SO", "NEE", "ZTS", "TGT", "DUK", "ICE", "BDX",
        "PNC", "CMCSA", "SCHW", "MDLZ", "TJX", "USB", "CL", "EMR", "APD", "COF", "FDX", "AON",
        "WM", "ECL", "ITW", "VRTX", "D", "NSC", "PGR", "ETN", "FIS", "PSA", "KLAC", "MCD", "ADP",
        "APTV", "AEP", "MCO", "SHW", "DD", "ROP", "SLB", "HUM", "BSX", "NOC", "EW",
    ])
});

static START_PRICE: f64 = 500.0;
static MIN_PRICE: f64 = 20.0;
static MAX_PRICE: f64 = 2000.0;

#[derive(Debug, Clone)]
pub struct StockQuote {
    pub ticker: String,
    pub price: f64,
    pub volume: u32,
    pub timestamp: u64,
}

// Методы для сериализации/десериализации
impl StockQuote {
    pub fn to_string(&self) -> String {
        format!(
            "{}|{:.2}|{}|{}",
            self.ticker, self.price, self.volume, self.timestamp
        )
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum QuoteGeneratorError {
    #[error("tracker {0} is not supported")]
    TrackerIsNotSupported(String),
}

pub struct QuoteGenerator {
    last_price: f64,
}

impl QuoteGenerator {
    pub fn new() -> Self {
        Self {
            last_price: START_PRICE,
        }
    }

    pub fn random_ticker() -> &'static str {
        let mut rng = rand::rng();
        SUPPORTED_TICKERS.iter().choose(&mut rng).unwrap()
    }

    pub fn generate_quote(&mut self, ticker: &str) -> Result<StockQuote, QuoteGeneratorError> {
        if !SUPPORTED_TICKERS.contains(ticker) {
            return Err(QuoteGeneratorError::TrackerIsNotSupported(
                ticker.to_string(),
            ));
        }

        let res = rand::random_range(-50.0..50.0);
        self.last_price += res;
        if self.last_price < MIN_PRICE {
            self.last_price = MIN_PRICE;
        } else if self.last_price > MAX_PRICE {
            self.last_price = MAX_PRICE;
        }

        let volume = match ticker {
            // Популярные акции имеют больший объём
            "AAPL" | "MSFT" | "TSLA" => 1000 + (rand::random::<f64>() * 5000.0) as u32,
            // Обычные акции - средний объём
            _ => 100 + (rand::random::<f64>() * 1000.0) as u32,
        };

        Ok(StockQuote {
            ticker: ticker.to_string(),
            price: self.last_price,
            volume,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_quote_successfully() {
        let mut quote_generator = QuoteGenerator::new();
        let quote = quote_generator.generate_quote("AAPL");
        assert!(quote.is_ok());
    }

    #[test]
    fn test_generate_quote_not_supported() {
        let mut quote_generator = QuoteGenerator::new();
        let quote = quote_generator.generate_quote("AA");
        assert!(quote.is_err());
        assert_eq!(
            quote.unwrap_err(),
            QuoteGeneratorError::TrackerIsNotSupported("AA".to_string())
        );
    }
}
