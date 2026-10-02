//! Instant answers in the quick panel: arithmetic and the time in a city.
//! Local code only, no model and no network. Enter copies the answer.

use chrono::{Datelike, Offset, TimeZone, Timelike};
use neko_protocol::{Glyph, Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// Leads the list: a computed answer is exactly what the query asked for.
const ANSWER_SCORE: f32 = 2.5;

pub struct AnswerProvider;

impl Provider for AnswerProvider {
    fn id(&self) -> &'static str {
        "answer"
    }

    fn section_label(&self) -> &'static str {
        "Answer"
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        let answer = calculate(query)
            .map(|value| (format_number(value), format!("= {}", query.trim())))
            .or_else(|| city_time(query, now_unix_ms));
        answer
            .map(|(title, subtitle)| vec![candidate(title, subtitle)])
            .unwrap_or_default()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        if crate::clipboard::write_to_pasteboard(id) {
            Ok(())
        } else {
            Err(ProviderError("Couldn’t copy the answer".into()))
        }
    }
}

fn candidate(title: String, subtitle: String) -> Candidate {
    Candidate {
        score: ANSWER_SCORE,
        item: SearchItem {
            id: title.clone(),
            kind: "answer".into(),
            title,
            subtitle: Some(subtitle),
            icon: Icon::Glyph(Glyph::Text),
            section_label: "Answer".into(),
            action_label: "Copy  ↵".into(),
            badge: None,
            accessory: None,
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
            meter: None,
            keeps_open: false,
            preview_markdown: false,
            speaker: None,
            images: Vec::new(),
            preview: None,
        },
    }
}

// ---------------------------------------------------------------- Arithmetic

/// A value for queries that are clearly arithmetic: at least one operator
/// and one digit. A bare number, a phone number or a word is not.
pub fn calculate(query: &str) -> Option<f64> {
    let text = query.trim().to_lowercase().replace(',', "").replace('×', "*").replace('÷', "/").replace('−', "-");
    let text = text.strip_suffix('=').unwrap_or(&text).trim().to_owned();
    if text.is_empty() || text.len() > 200 || !text.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    // "15% of 80" and "20 percent of 50".
    let text = text.replace(" percent", "%");
    if let Some((left, right)) = text.split_once("% of ") {
        let (a, b) = (Parser::new(left).full()?, Parser::new(right).full()?);
        return Some(a / 100.0 * b);
    }
    let has_operator = text.chars().enumerate().any(|(i, c)| "+*/^%".contains(c) || (c == '-' && i > 0) || c == 'x')
        || text.contains("sqrt");
    if !has_operator {
        return None;
    }
    let value = Parser::new(&text.replace('x', "*").replace("sqrt*", "sqrt")).full()?;
    value.is_finite().then_some(value)
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { chars: text.trim().chars().peekable() }
    }
    fn full(mut self) -> Option<f64> {
        let value = self.sum()?;
        self.skip();
        self.chars.peek().is_none().then_some(value)
    }
    fn skip(&mut self) {
        while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
            self.chars.next();
        }
    }
    fn eat(&mut self, c: char) -> bool {
        self.skip();
        if self.chars.peek() == Some(&c) {
            self.chars.next();
            true
        } else {
            false
        }
    }
    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.eat('+') {
                value += self.product()?;
            } else if self.eat('-') {
                value -= self.product()?;
            } else {
                return Some(value);
            }
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            if self.eat('*') {
                value *= self.power()?;
            } else if self.eat('/') {
                let divisor = self.power()?;
                if divisor == 0.0 {
                    return None;
                }
                value /= divisor;
            } else {
                return Some(value);
            }
        }
    }
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.eat('^') {
            let exponent = self.power()?; // right-associative
            let value = base.powf(exponent);
            return value.is_finite().then_some(value);
        }
        Some(base)
    }
    fn unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        let mut value = self.atom()?;
        while self.eat('%') {
            value /= 100.0;
        }
        Some(value)
    }
    fn atom(&mut self) -> Option<f64> {
        self.skip();
        if self.eat('(') {
            let value = self.sum()?;
            return self.eat(')').then_some(value);
        }
        if self.chars.peek() == Some(&'s') {
            for expected in "sqrt".chars() {
                if self.chars.next() != Some(expected) {
                    return None;
                }
            }
            let value = self.atom()?;
            return (value >= 0.0).then(|| value.sqrt());
        }
        let mut number = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                number.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        number.parse().ok()
    }
}

pub fn format_number(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{:.10}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}

// ---------------------------------------------------------------- City time

const ALIASES: &[(&str, &str)] = &[
    ("nyc", "America/New_York"), ("new york", "America/New_York"), ("sf", "America/Los_Angeles"),
    ("san francisco", "America/Los_Angeles"), ("la", "America/Los_Angeles"), ("seattle", "America/Los_Angeles"),
    ("bangalore", "Asia/Kolkata"), ("bengaluru", "Asia/Kolkata"), ("mumbai", "Asia/Kolkata"), ("delhi", "Asia/Kolkata"),
    ("india", "Asia/Kolkata"), ("ist", "Asia/Kolkata"), ("uk", "Europe/London"), ("japan", "Asia/Tokyo"),
    ("china", "Asia/Shanghai"), ("beijing", "Asia/Shanghai"), ("utc", "UTC"), ("gmt", "UTC"),
    ("boston", "America/New_York"), ("austin", "America/Chicago"), ("toronto", "America/Toronto"),
];

/// "time in tokyo", "tokyo time", "what time is it in london".
pub fn city_time(query: &str, now_unix_ms: i64) -> Option<(String, String)> {
    let lower = query.trim().trim_end_matches('?').to_lowercase();
    let city = lower
        .strip_prefix("what time is it in ")
        .or_else(|| lower.strip_prefix("time in "))
        .or_else(|| lower.strip_suffix(" time"))
        .map(str::trim)
        .filter(|c| !c.is_empty())?;
    let zone: chrono_tz::Tz = ALIASES
        .iter()
        .find(|(alias, _)| *alias == city)
        .and_then(|(_, name)| name.parse().ok())
        .or_else(|| {
            chrono_tz::TZ_VARIANTS.iter().copied().find(|tz| {
                tz.name().rsplit('/').next().is_some_and(|last| last.replace('_', " ").to_lowercase() == city)
            })
        })?;
    let now = chrono::Utc.timestamp_millis_opt(now_unix_ms).single()?.with_timezone(&zone);
    let offset = now.offset().fix().local_minus_utc();
    let (pm, hour) = now.hour12();
    let sign = if offset < 0 { '-' } else { '+' };
    let (h, m) = (offset.abs() / 3600, offset.abs() % 3600 / 60);
    let utc = if m == 0 { format!("UTC{sign}{h}") } else { format!("UTC{sign}{h}:{m:02}") };
    Some((
        format!("{hour}:{:02} {}", now.minute(), if pm { "PM" } else { "AM" }),
        format!("{} {} {} · {} ({})", now.format("%a"), now.day(), now.format("%b"), zone.name(), utc),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_is_answered_and_plain_text_is_not() {
        assert_eq!(calculate("2+2*3"), Some(8.0));
        assert_eq!(calculate("(2+3)^2"), Some(25.0));
        assert_eq!(calculate("2^3^2"), Some(512.0));
        assert_eq!(calculate("15% of 80"), Some(12.0));
        assert_eq!(calculate("1,200 / 4"), Some(300.0));
        assert_eq!(calculate("3 x 4"), Some(12.0));
        assert_eq!(calculate("-5 + 2"), Some(-3.0));
        assert_eq!(calculate("sqrt(16) + 1"), Some(5.0));
        assert_eq!(calculate("50%"), Some(0.5));
        assert_eq!(calculate("1/0"), None);
        assert_eq!(calculate("42"), None, "a bare number is not a question");
        assert_eq!(calculate("-42"), None);
        assert_eq!(calculate("safari"), None);
        assert_eq!(calculate("iphone 15"), None);
        assert_eq!(calculate("2 +"), None);
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(1.0 / 3.0), "0.3333333333");
    }

    #[test]
    fn city_times_use_the_right_zone() {
        // 2026-10-02 12:00:00 UTC.
        let noon = 1_790_942_400_000;
        let (time, detail) = city_time("time in tokyo", noon).unwrap();
        assert_eq!(time, "9:00 PM");
        assert!(detail.contains("Asia/Tokyo (UTC+9)"), "{detail}");
        let (time, detail) = city_time("Bangalore time", noon).unwrap();
        assert_eq!(time, "5:30 PM");
        assert!(detail.contains("UTC+5:30"));
        assert!(city_time("what time is it in new york?", noon).unwrap().1.contains("America/New_York"));
        assert!(city_time("time in atlantis", noon).is_none());
        assert!(city_time("showtime", noon).is_none());
        assert_eq!(AnswerProvider.search("time in london", noon).len(), 1);
        assert!(AnswerProvider.search("weather", noon).is_empty());
    }
}
