//! Recursive-descent parser for the compact running-step notation.
//!
//! ```text
//! workout  := item (SEP item)* SEP?          SEP := ';' | ','
//! item     := repeat | step
//! repeat   := INT ('x'|'X'|'×') '(' step (SEP step)* SEP? ')'
//! step     := kind end [target] [note]
//! kind     := wu|warmup | cd|cooldown | int|interval | rec|recovery | rest | other
//! end      := 'lap' | distance | duration
//! duration := [N'h'][N('min'|'m')][N's']  |  m:ss  |  h:mm:ss
//! distance := DEC ('km'|'mi'|'mt'|'meters')
//! target   := '@' ( ('z'|'Z')[1-5]  |  PACE['-'PACE]'/'('km'|'mi') )
//! note     := '[' text ']'
//! ```
//!
//! Pure, no I/O. `pos` on errors and AST nodes is a 1-based column into the
//! original input string.

/// A parsed workout item: a single step or a repeat group.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Step(Step),
    Repeat {
        count: u32,
        steps: Vec<Step>,
        pos: usize,
    },
}

/// A single executable step.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub kind: StepKind,
    pub end: End,
    pub target: Target,
    pub note: Option<String>,
    pub pos: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Warmup,
    Cooldown,
    Interval,
    Recovery,
    Rest,
    Other,
}

/// How a step ends.
#[derive(Debug, Clone, PartialEq)]
pub enum End {
    /// Duration in seconds.
    Time(u32),
    Distance {
        meters: f64,
        unit: DistUnit,
    },
    Lap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistUnit {
    Kilometer,
    Mile,
    Meter,
}

/// A step's intensity target.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    None,
    HrZone(u8),
    /// Pace band in meters/second. `fast_mps` is the faster (higher) bound.
    Pace {
        fast_mps: f64,
        slow_mps: f64,
    },
}

/// A parse error with a 1-based column position into the source string.
#[derive(Debug, Clone, PartialEq)]
pub struct NotationError {
    pub pos: usize,
    pub msg: String,
}

impl NotationError {
    /// Render a two-line source-and-caret display, prefixed with a one-line
    /// summary naming `flag` (e.g. `--steps`) and the error column.
    ///
    /// ```text
    /// invalid --steps at column 17: expected duration, distance or 'lap'
    ///   wu 10m @z2; int @z4
    ///                   ^
    /// ```
    pub fn to_display(&self, flag: &str, source: &str) -> String {
        format!(
            "invalid {} at column {}: {}\n  {}\n  {}^",
            flag,
            self.pos,
            self.msg,
            source,
            " ".repeat(self.pos.saturating_sub(1))
        )
    }
}

impl std::fmt::Display for NotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "column {}: {}", self.pos, self.msg)
    }
}

const PACE_BAND_SECS: u32 = 5;
const MIN_DURATION_SECS: u32 = 1;
const MAX_DURATION_SECS: u32 = 6 * 3600;
const MAX_DISTANCE_METERS: f64 = 100_000.0;
const MIN_PACE_SECS: u32 = 120;
const MAX_PACE_SECS: u32 = 900;
const MIN_REPEAT_COUNT: u32 = 1;
const MAX_REPEAT_COUNT: u32 = 99;
const MIN_HR_ZONE: u32 = 1;
const MAX_HR_ZONE: u32 = 5;
const METERS_PER_MILE: f64 = 1609.344;
const METERS_PER_KM: f64 = 1000.0;

/// Parse a full `--steps` string into a list of workout items.
pub fn parse(input: &str) -> Result<Vec<Item>, NotationError> {
    let mut parser = Parser {
        chars: input.chars().collect(),
        pos: 0,
    };
    let items = parser.parse_items()?;
    if !parser.at_end() {
        return Err(parser.error("unexpected trailing input"));
    }
    Ok(items)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    /// 1-based column for `pos`.
    fn column(&self, pos: usize) -> usize {
        pos + 1
    }

    fn error(&self, msg: impl Into<String>) -> NotationError {
        self.error_at(self.pos, msg)
    }

    fn error_at(&self, pos: usize, msg: impl Into<String>) -> NotationError {
        NotationError {
            pos: self.column(pos),
            msg: msg.into(),
        }
    }

    fn expect_char(&mut self, expected: char) -> Result<(), NotationError> {
        match self.peek() {
            Some(c) if c == expected => {
                self.advance();
                Ok(())
            }
            _ => Err(self.error(format!("expected '{}'", expected))),
        }
    }

    /// Matches an exact, case-sensitive keyword not followed by another
    /// alphanumeric character (so `lap` doesn't match a prefix of `laps`).
    fn match_keyword(&mut self, kw: &str) -> bool {
        let kw_chars: Vec<char> = kw.chars().collect();
        if self.pos + kw_chars.len() > self.chars.len() {
            return false;
        }
        if self.chars[self.pos..self.pos + kw_chars.len()] != kw_chars[..] {
            return false;
        }
        let boundary_ok = self
            .chars
            .get(self.pos + kw_chars.len())
            .is_none_or(|c| !c.is_alphanumeric());
        if boundary_ok {
            self.pos += kw_chars.len();
        }
        boundary_ok
    }

    fn read_alpha(&mut self) -> String {
        let mut s = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            s.push(self.advance().unwrap());
        }
        s
    }

    fn read_uint_str(&mut self) -> Option<String> {
        let mut s = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            s.push(self.advance().unwrap());
        }
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    fn read_uint(&mut self) -> Option<u32> {
        self.read_uint_str().and_then(|s| s.parse().ok())
    }

    fn read_decimal_str(&mut self) -> Option<String> {
        let mut s = self.read_uint_str().unwrap_or_default();
        if s.is_empty() && self.peek() != Some('.') {
            return None;
        }
        if self.peek() == Some('.') {
            s.push('.');
            self.advance();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                s.push(self.advance().unwrap());
            }
        }
        if s.is_empty() || s == "." {
            None
        } else {
            Some(s)
        }
    }

    /// True if a colon-separated time (`m:ss` or `h:mm:ss`) starts here,
    /// without consuming any input.
    fn peek_is_colon_time(&self) -> bool {
        let mut i = self.pos;
        while matches!(self.chars.get(i), Some(c) if c.is_ascii_digit()) {
            i += 1;
        }
        i > self.pos && self.chars.get(i) == Some(&':')
    }

    fn parse_items(&mut self) -> Result<Vec<Item>, NotationError> {
        self.skip_ws();
        if self.at_end() {
            return Err(self.error("expected at least one step"));
        }
        let mut items = vec![self.parse_item()?];
        loop {
            self.skip_ws();
            if self.at_end() {
                break;
            }
            match self.peek() {
                Some(';') | Some(',') => {
                    self.advance();
                    self.skip_ws();
                    if self.at_end() {
                        break;
                    }
                    items.push(self.parse_item()?);
                }
                _ => return Err(self.error("expected ';' or ',' between steps")),
            }
        }
        Ok(items)
    }

    fn peek_is_repeat_header(&self) -> bool {
        let mut i = self.pos;
        while matches!(self.chars.get(i), Some(c) if c.is_ascii_digit()) {
            i += 1;
        }
        if i == self.pos {
            return false;
        }
        matches!(self.chars.get(i), Some('x') | Some('X') | Some('×'))
    }

    fn parse_item(&mut self) -> Result<Item, NotationError> {
        if self.peek_is_repeat_header() {
            self.parse_repeat()
        } else {
            Ok(Item::Step(self.parse_step()?))
        }
    }

    fn parse_repeat(&mut self) -> Result<Item, NotationError> {
        let start = self.pos;
        let count_str = self
            .read_uint_str()
            .ok_or_else(|| self.error("expected repeat count"))?;
        let count: u32 = count_str
            .parse()
            .map_err(|_| self.error_at(start, "invalid repeat count"))?;
        match self.peek() {
            Some('x') | Some('X') | Some('×') => {
                self.advance();
            }
            _ => return Err(self.error("expected 'x' after repeat count")),
        }
        self.skip_ws();
        self.expect_char('(')?;
        self.skip_ws();

        let mut steps = Vec::new();
        loop {
            if self.peek() == Some(')') {
                break;
            }
            if self.peek_is_repeat_header() {
                return Err(self.error("nested repeat groups are not allowed"));
            }
            steps.push(self.parse_step()?);
            self.skip_ws();
            match self.peek() {
                Some(';') | Some(',') => {
                    self.advance();
                    self.skip_ws();
                }
                Some(')') => break,
                _ => return Err(self.error("expected ';', ',' or ')'")),
            }
        }
        self.expect_char(')')
            .map_err(|_| self.error_at(self.pos, "expected ')'"))?;

        if steps.is_empty() {
            return Err(self.error_at(start, "repeat group must contain at least one step"));
        }
        if !(MIN_REPEAT_COUNT..=MAX_REPEAT_COUNT).contains(&count) {
            return Err(self.error_at(
                start,
                format!(
                    "repeat count must be between {} and {}",
                    MIN_REPEAT_COUNT, MAX_REPEAT_COUNT
                ),
            ));
        }

        Ok(Item::Repeat {
            count,
            steps,
            pos: self.column(start),
        })
    }

    fn parse_step(&mut self) -> Result<Step, NotationError> {
        let start = self.pos;
        if self.peek_is_repeat_header() {
            return Err(self.error("nested repeat groups are not allowed"));
        }

        let kind = self.parse_kind()?;
        self.skip_ws();
        let end = self.parse_end()?;

        let mut target = Target::None;
        let mut note = None;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('@') if target == Target::None => {
                    target = self.parse_target()?;
                }
                Some('[') if note.is_none() => {
                    note = Some(self.parse_note()?);
                }
                _ => break,
            }
        }

        Ok(Step {
            kind,
            end,
            target,
            note,
            pos: self.column(start),
        })
    }

    fn parse_kind(&mut self) -> Result<StepKind, NotationError> {
        let start = self.pos;
        let word = self.read_alpha();
        match word.as_str() {
            "wu" | "warmup" => Ok(StepKind::Warmup),
            "cd" | "cooldown" => Ok(StepKind::Cooldown),
            "int" | "interval" => Ok(StepKind::Interval),
            "rec" | "recovery" => Ok(StepKind::Recovery),
            "rest" => Ok(StepKind::Rest),
            "other" => Ok(StepKind::Other),
            "" => Err(self.error_at(start, "expected a step kind")),
            other => Err(self.error_at(start, format!("unknown step kind '{}'", other))),
        }
    }

    fn parse_end(&mut self) -> Result<End, NotationError> {
        self.skip_ws();
        let start = self.pos;
        if self.match_keyword("lap") {
            return Ok(End::Lap);
        }
        if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            if self.peek_is_colon_time() {
                return self.parse_colon_time(start);
            }
            return self.parse_number_with_unit(start);
        }
        Err(self.error_at(start, "expected duration, distance or 'lap'"))
    }

    fn parse_colon_time(&mut self, start: usize) -> Result<End, NotationError> {
        let mut parts = Vec::new();
        loop {
            let seg_start = self.pos;
            let n = self
                .read_uint()
                .ok_or_else(|| self.error_at(seg_start, "expected a number"))?;
            parts.push(n);
            if self.peek() == Some(':') {
                self.advance();
            } else {
                break;
            }
        }
        let total = match parts.len() {
            2 => parts[0] * 60 + parts[1],
            3 => parts[0] * 3600 + parts[1] * 60 + parts[2],
            _ => {
                return Err(
                    self.error_at(start, "invalid duration format, expected m:ss or h:mm:ss")
                )
            }
        };
        self.validate_duration(total, start)?;
        Ok(End::Time(total))
    }

    fn parse_number_with_unit(&mut self, start: usize) -> Result<End, NotationError> {
        let num_str = self
            .read_decimal_str()
            .ok_or_else(|| self.error_at(start, "expected a number"))?;
        let unit_start = self.pos;
        let suffix = self.read_alpha();
        match suffix.as_str() {
            "h" => {
                let hours: u32 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid hours value"))?;
                let mut total = hours.saturating_mul(3600);
                total = total.saturating_add(self.parse_duration_tail()?);
                self.validate_duration(total, start)?;
                Ok(End::Time(total))
            }
            "min" | "m" => {
                let minutes: u32 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid minutes value"))?;
                if suffix == "m" && minutes >= 100 {
                    return Err(self.error_at(
                        start,
                        "400m means 400 minutes; for meters write 400mt or 0.4km",
                    ));
                }
                let mut total = minutes.saturating_mul(60);
                total = total.saturating_add(self.parse_duration_tail()?);
                self.validate_duration(total, start)?;
                Ok(End::Time(total))
            }
            "s" => {
                let secs: u32 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid seconds value"))?;
                self.validate_duration(secs, start)?;
                Ok(End::Time(secs))
            }
            "km" => {
                let km: f64 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid distance value"))?;
                let meters = km * METERS_PER_KM;
                self.validate_distance(meters, start)?;
                Ok(End::Distance {
                    meters,
                    unit: DistUnit::Kilometer,
                })
            }
            "mi" => {
                let mi: f64 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid distance value"))?;
                let meters = mi * METERS_PER_MILE;
                self.validate_distance(meters, start)?;
                Ok(End::Distance {
                    meters,
                    unit: DistUnit::Mile,
                })
            }
            "mt" | "meters" => {
                let meters: f64 = num_str
                    .parse()
                    .map_err(|_| self.error_at(start, "invalid distance value"))?;
                self.validate_distance(meters, start)?;
                Ok(End::Distance {
                    meters,
                    unit: DistUnit::Meter,
                })
            }
            other => Err(self.error_at(
                unit_start,
                format!(
                    "unknown unit '{}'; expected h, min, m, s, km, mi, mt or meters",
                    other
                ),
            )),
        }
    }

    /// Consumes any additional `N(min|m|s)` segments following an initial
    /// `h` or `min`/`m` segment, e.g. the `30m` in `1h30m`.
    fn parse_duration_tail(&mut self) -> Result<u32, NotationError> {
        let mut total = 0u32;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            let seg_start = self.pos;
            let num_str = self.read_uint_str().unwrap();
            let suffix = self.read_alpha();
            match suffix.as_str() {
                "min" | "m" => {
                    let val: u32 = num_str
                        .parse()
                        .map_err(|_| self.error_at(seg_start, "invalid minutes value"))?;
                    if suffix == "m" && val >= 100 {
                        return Err(self.error_at(
                            seg_start,
                            "400m means 400 minutes; for meters write 400mt or 0.4km",
                        ));
                    }
                    total = total.saturating_add(val.saturating_mul(60));
                }
                "s" => {
                    let val: u32 = num_str
                        .parse()
                        .map_err(|_| self.error_at(seg_start, "invalid seconds value"))?;
                    total = total.saturating_add(val);
                }
                other => {
                    return Err(self.error_at(
                        seg_start,
                        format!("unexpected duration segment '{}{}'", num_str, other),
                    ))
                }
            }
        }
        Ok(total)
    }

    fn validate_duration(&self, secs: u32, start: usize) -> Result<(), NotationError> {
        if !(MIN_DURATION_SECS..=MAX_DURATION_SECS).contains(&secs) {
            return Err(self.error_at(start, "duration must be between 1 second and 6 hours"));
        }
        Ok(())
    }

    fn validate_distance(&self, meters: f64, start: usize) -> Result<(), NotationError> {
        if !(meters > 0.0 && meters <= MAX_DISTANCE_METERS) {
            return Err(self.error_at(start, "distance must be greater than 0 and at most 100 km"));
        }
        Ok(())
    }

    fn parse_target(&mut self) -> Result<Target, NotationError> {
        self.expect_char('@')?;
        let start = self.pos;
        match self.peek() {
            Some('z') | Some('Z') => {
                self.advance();
                match self.peek().and_then(|c| c.to_digit(10)) {
                    Some(z) if (MIN_HR_ZONE..=MAX_HR_ZONE).contains(&z) => {
                        self.advance();
                        Ok(Target::HrZone(z as u8))
                    }
                    _ => Err(self.error_at(start, "expected HR zone 1-5 after 'z'")),
                }
            }
            _ => self.parse_pace_target(start),
        }
    }

    fn parse_pace_time(&mut self, context: usize) -> Result<u32, NotationError> {
        let min = self
            .read_uint()
            .ok_or_else(|| self.error_at(context, "expected pace, e.g. 4:30"))?;
        self.expect_char(':')
            .map_err(|_| self.error_at(context, "expected ':' in pace, e.g. 4:30"))?;
        let sec_start = self.pos;
        let sec = self
            .read_uint()
            .ok_or_else(|| self.error_at(sec_start, "expected seconds after ':'"))?;
        if sec >= 60 {
            return Err(self.error_at(sec_start, "seconds must be less than 60"));
        }
        Ok(min * 60 + sec)
    }

    fn parse_pace_target(&mut self, start: usize) -> Result<Target, NotationError> {
        let p1 = self.parse_pace_time(start)?;
        let p2 = if self.peek() == Some('-') {
            self.advance();
            Some(self.parse_pace_time(self.pos)?)
        } else {
            None
        };
        self.expect_char('/')
            .map_err(|_| self.error_at(self.pos, "expected '/km' or '/mi' after pace"))?;
        let unit_start = self.pos;
        let unit = self.read_alpha();
        let dist_for_unit = match unit.as_str() {
            "km" => METERS_PER_KM,
            "mi" => METERS_PER_MILE,
            _ => {
                return Err(self.error_at(unit_start, "expected 'km' or 'mi' after pace"));
            }
        };

        let (fast_secs, slow_secs) = match p2 {
            Some(p2) => {
                if p1 <= p2 {
                    (p1, p2)
                } else {
                    (p2, p1)
                }
            }
            None => (
                p1.saturating_sub(PACE_BAND_SECS),
                p1.saturating_add(PACE_BAND_SECS),
            ),
        };

        self.validate_pace(fast_secs, start)?;
        self.validate_pace(slow_secs, start)?;

        Ok(Target::Pace {
            fast_mps: dist_for_unit / fast_secs as f64,
            slow_mps: dist_for_unit / slow_secs as f64,
        })
    }

    fn validate_pace(&self, secs: u32, start: usize) -> Result<(), NotationError> {
        if !(MIN_PACE_SECS..=MAX_PACE_SECS).contains(&secs) {
            return Err(self.error_at(start, "pace must be between 2:00 and 15:00"));
        }
        Ok(())
    }

    fn parse_note(&mut self) -> Result<String, NotationError> {
        let start = self.pos;
        self.expect_char('[')?;
        let mut text = String::new();
        loop {
            match self.peek() {
                Some(']') => {
                    self.advance();
                    return Ok(text);
                }
                Some(c) => {
                    text.push(c);
                    self.advance();
                }
                None => return Err(self.error_at(start, "unterminated note, expected ']'")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(input: &str) -> Vec<Item> {
        parse(input).unwrap_or_else(|e| panic!("failed to parse {:?}: {}", input, e))
    }

    fn parse_err(input: &str) -> NotationError {
        parse(input).expect_err(&format!("expected {:?} to fail", input))
    }

    #[test]
    fn parses_example_one() {
        let items = parse_ok("wu 10m @z2; 3x(int 4m @z4, rec 3m @z3); cd 10m @z1");
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0],
            Item::Step(Step {
                kind: StepKind::Warmup,
                end: End::Time(600),
                target: Target::HrZone(2),
                note: None,
                pos: 1,
            })
        );
        match &items[1] {
            Item::Repeat { count, steps, .. } => {
                assert_eq!(*count, 3);
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].kind, StepKind::Interval);
                assert_eq!(steps[0].end, End::Time(240));
                assert_eq!(steps[0].target, Target::HrZone(4));
                assert_eq!(steps[1].kind, StepKind::Recovery);
                assert_eq!(steps[1].end, End::Time(180));
                assert_eq!(steps[1].target, Target::HrZone(3));
            }
            _ => panic!("expected repeat"),
        }
        assert_eq!(
            items[2],
            Item::Step(Step {
                kind: StepKind::Cooldown,
                end: End::Time(600),
                target: Target::HrZone(1),
                note: None,
                pos: 41,
            })
        );
    }

    #[test]
    fn parses_example_two() {
        let items = parse_ok("wu 2km; int 5km @4:30-4:40/km; cd 1km");
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0],
            Item::Step(Step {
                kind: StepKind::Warmup,
                end: End::Distance {
                    meters: 2000.0,
                    unit: DistUnit::Kilometer
                },
                target: Target::None,
                note: None,
                pos: 1,
            })
        );
        match &items[1] {
            Item::Step(step) => {
                assert_eq!(step.kind, StepKind::Interval);
                assert_eq!(
                    step.end,
                    End::Distance {
                        meters: 5000.0,
                        unit: DistUnit::Kilometer
                    }
                );
                match step.target {
                    Target::Pace { fast_mps, slow_mps } => {
                        assert!((fast_mps - 1000.0 / 270.0).abs() < 1e-9);
                        assert!((slow_mps - 1000.0 / 280.0).abs() < 1e-9);
                    }
                    _ => panic!("expected pace target"),
                }
            }
            _ => panic!("expected step"),
        }
        assert_eq!(
            items[2],
            Item::Step(Step {
                kind: StepKind::Cooldown,
                end: End::Distance {
                    meters: 1000.0,
                    unit: DistUnit::Kilometer
                },
                target: Target::None,
                note: None,
                pos: 32,
            })
        );
    }

    #[test]
    fn duration_forms() {
        assert_eq!(single_end("wu 90s"), End::Time(90));
        assert_eq!(single_end("wu 1h30m"), End::Time(5400));
        assert_eq!(single_end("wu 2m30s"), End::Time(150));
        assert_eq!(single_end("wu 4:00"), End::Time(240));
        assert_eq!(single_end("wu 1:05:00"), End::Time(3900));
        assert_eq!(single_end("wu 10min"), End::Time(600));
    }

    #[test]
    fn distance_forms() {
        assert_eq!(
            single_end("wu 400mt"),
            End::Distance {
                meters: 400.0,
                unit: DistUnit::Meter
            }
        );
        assert_eq!(
            single_end("wu 0.4km"),
            End::Distance {
                meters: 400.0,
                unit: DistUnit::Kilometer
            }
        );
        let mi = single_end("wu 1.5mi");
        match mi {
            End::Distance { meters, unit } => {
                assert!((meters - 1.5 * METERS_PER_MILE).abs() < 1e-9);
                assert_eq!(unit, DistUnit::Mile);
            }
            _ => panic!("expected distance"),
        }
        assert_eq!(single_end("wu lap"), End::Lap);
    }

    #[test]
    fn single_pace_band_is_plus_minus_five_seconds() {
        let items = parse_ok("int 5km @4:30/km");
        match &items[0] {
            Item::Step(step) => match step.target {
                Target::Pace { fast_mps, slow_mps } => {
                    assert!((fast_mps - 1000.0 / 265.0).abs() < 1e-9);
                    assert!((slow_mps - 1000.0 / 275.0).abs() < 1e-9);
                }
                _ => panic!("expected pace target"),
            },
            _ => panic!("expected step"),
        }
    }

    #[test]
    fn reversed_pace_range_normalizes() {
        let items = parse_ok("int 5km @4:40-4:30/km");
        match &items[0] {
            Item::Step(step) => match step.target {
                Target::Pace { fast_mps, slow_mps } => {
                    assert!((fast_mps - 1000.0 / 270.0).abs() < 1e-9);
                    assert!((slow_mps - 1000.0 / 280.0).abs() < 1e-9);
                }
                _ => panic!("expected pace target"),
            },
            _ => panic!("expected step"),
        }
    }

    #[test]
    fn pace_per_mile() {
        let items = parse_ok("int 3mi @7:00-7:20/mi");
        match &items[0] {
            Item::Step(step) => match step.target {
                Target::Pace { fast_mps, slow_mps } => {
                    assert!((fast_mps - METERS_PER_MILE / 420.0).abs() < 1e-9);
                    assert!((slow_mps - METERS_PER_MILE / 440.0).abs() < 1e-9);
                }
                _ => panic!("expected pace target"),
            },
            _ => panic!("expected step"),
        }
    }

    #[test]
    fn note_is_captured() {
        let items = parse_ok("wu 10m @z2 [easy shakeout]");
        match &items[0] {
            Item::Step(step) => assert_eq!(step.note.as_deref(), Some("easy shakeout")),
            _ => panic!("expected step"),
        }
    }

    #[test]
    fn trailing_separators_are_ignored() {
        assert_eq!(parse_ok("wu 10m;").len(), 1);
        assert_eq!(parse_ok("wu 10m,").len(), 1);
        assert_eq!(parse_ok("3x(int 4m; rec 3m;)").len(), 1);
        assert_eq!(parse_ok("3x(int 4m, rec 3m,)").len(), 1);
    }

    #[test]
    fn comma_and_semicolon_are_interchangeable() {
        let a = parse_ok("wu 10m, cd 10m");
        let b = parse_ok("wu 10m; cd 10m");
        assert_eq!(a.len(), b.len());
    }

    fn single_end(input: &str) -> End {
        match &parse_ok(input)[0] {
            Item::Step(step) => step.end.clone(),
            _ => panic!("expected step"),
        }
    }

    #[test]
    fn error_bare_minutes_guard() {
        let err = parse_err("wu 400m");
        assert!(err.msg.contains("400mt or 0.4km"));
    }

    #[test]
    fn error_invalid_hr_zone() {
        let err = parse_err("wu 10m @z6");
        assert!(err.msg.contains("HR zone"));
    }

    #[test]
    fn error_empty_repeat_group() {
        parse_err("3x()");
    }

    #[test]
    fn error_nested_repeat() {
        let err = parse_err("3x(2x(int 4m))");
        assert!(err.msg.contains("nested repeat"));
    }

    #[test]
    fn error_unknown_kind() {
        let err = parse_err("foo 10m");
        assert!(err.msg.contains("unknown step kind"));
    }

    #[test]
    fn error_missing_close_paren() {
        parse_err("3x(int 4m");
    }

    #[test]
    fn error_missing_end_condition() {
        let err = parse_err("wu 10m @z2; int @z4");
        assert_eq!(err.pos, 17);
        assert!(err.msg.contains("expected duration, distance or 'lap'"));
    }

    #[test]
    fn error_zero_repeat_count() {
        parse_err("0x(int 4m)");
    }

    #[test]
    fn error_empty_string() {
        parse_err("");
    }

    #[test]
    fn error_display_renders_caret() {
        let err = parse_err("wu 10m @z2; int @z4");
        let rendered = err.to_display("--steps", "wu 10m @z2; int @z4");
        assert_eq!(
            rendered,
            "invalid --steps at column 17: expected duration, distance or 'lap'\n  wu 10m @z2; int @z4\n                  ^"
        );
    }
}
