use alloc::vec::Vec;

use super::svg_path_error::{SvgPathError, invalid_number};
use super::svg_path_group::SvgPathGeometry;

pub fn parse_polyline(points: &str) -> Result<SvgPathGeometry, SvgPathError> {
    let numbers = parse_number_list(points)?;
    if numbers.len() < 4 || numbers.len() % 2 != 0 {
        return Err(SvgPathError::InvalidPolyline);
    }
    Ok(SvgPathGeometry::Polyline(
        numbers.chunks(2).map(|pair| [pair[0], pair[1]]).collect(),
    ))
}

pub fn parse_path_data(data: &str) -> Result<SvgPathGeometry, SvgPathError> {
    let mut parser = PathDataParser::new(data);
    parser.parse()
}

fn parse_number_list(input: &str) -> Result<Vec<f32>, SvgPathError> {
    let mut numbers = Vec::new();
    for value in input
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|value| !value.is_empty())
    {
        numbers.push(parse_f32(value)?);
    }
    Ok(numbers)
}

struct PathDataParser<'a> {
    input: &'a str,
    index: usize,
    command: Option<char>,
    current: [f32; 2],
    subpath_start: [f32; 2],
    points: Vec<[f32; 2]>,
}

impl<'a> PathDataParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            index: 0,
            command: None,
            current: [0.0, 0.0],
            subpath_start: [0.0, 0.0],
            points: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<SvgPathGeometry, SvgPathError> {
        while self.skip_separators() {
            if let Some(command) = self.peek_char().filter(|c| c.is_ascii_alphabetic()) {
                self.index += command.len_utf8();
                self.command = Some(command);
                if matches!(command, 'Z' | 'z') {
                    self.line_to(self.subpath_start);
                    continue;
                }
            }

            let Some(command) = self.command else {
                return Err(SvgPathError::InvalidAttribute { name: "d" });
            };

            match command {
                'M' | 'm' => self.parse_move(command == 'm')?,
                'L' | 'l' => self.parse_line(command == 'l')?,
                'H' | 'h' => self.parse_horizontal(command == 'h')?,
                'V' | 'v' => self.parse_vertical(command == 'v')?,
                'Z' | 'z' => {}
                other => return Err(SvgPathError::UnsupportedCommand(other)),
            }
        }

        if self.points.len() < 2 {
            return Err(SvgPathError::InvalidAttribute { name: "d" });
        }
        Ok(SvgPathGeometry::Polyline(self.points.clone()))
    }

    fn parse_move(&mut self, relative: bool) -> Result<(), SvgPathError> {
        let first = self.parse_point(relative)?;
        self.current = first;
        self.subpath_start = first;
        self.points.push(first);
        self.command = Some(if relative { 'l' } else { 'L' });

        while self.has_number_ahead() {
            let point = self.parse_point(relative)?;
            self.line_to(point);
        }
        Ok(())
    }

    fn parse_line(&mut self, relative: bool) -> Result<(), SvgPathError> {
        while self.has_number_ahead() {
            let point = self.parse_point(relative)?;
            self.line_to(point);
        }
        Ok(())
    }

    fn parse_horizontal(&mut self, relative: bool) -> Result<(), SvgPathError> {
        while self.has_number_ahead() {
            let x = self.parse_number()?;
            let next_x = if relative { self.current[0] + x } else { x };
            self.line_to([next_x, self.current[1]]);
        }
        Ok(())
    }

    fn parse_vertical(&mut self, relative: bool) -> Result<(), SvgPathError> {
        while self.has_number_ahead() {
            let y = self.parse_number()?;
            let next_y = if relative { self.current[1] + y } else { y };
            self.line_to([self.current[0], next_y]);
        }
        Ok(())
    }

    fn parse_point(&mut self, relative: bool) -> Result<[f32; 2], SvgPathError> {
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        if relative {
            Ok([self.current[0] + x, self.current[1] + y])
        } else {
            Ok([x, y])
        }
    }

    fn line_to(&mut self, point: [f32; 2]) {
        self.current = point;
        if self.points.last().copied() != Some(point) {
            self.points.push(point);
        }
    }

    fn has_number_ahead(&mut self) -> bool {
        self.skip_separators()
            && self
                .peek_char()
                .is_some_and(|c| c == '-' || c == '+' || c == '.' || c.is_ascii_digit())
    }

    fn parse_number(&mut self) -> Result<f32, SvgPathError> {
        self.skip_separators();
        let start = self.index;
        let mut seen_digit = false;
        if self.peek_char().is_some_and(|c| c == '-' || c == '+') {
            self.index += 1;
        }
        while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
            self.index += 1;
            seen_digit = true;
        }
        if self.peek_char() == Some('.') {
            self.index += 1;
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.index += 1;
                seen_digit = true;
            }
        }
        if self.peek_char().is_some_and(|c| c == 'e' || c == 'E') {
            let exponent = self.index;
            self.index += 1;
            if self.peek_char().is_some_and(|c| c == '-' || c == '+') {
                self.index += 1;
            }
            let exponent_start = self.index;
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.index += 1;
            }
            if exponent_start == self.index {
                self.index = exponent;
            }
        }
        if !seen_digit {
            return Err(SvgPathError::InvalidAttribute { name: "number" });
        }
        parse_f32(&self.input[start..self.index])
    }

    fn skip_separators(&mut self) -> bool {
        while self
            .peek_char()
            .is_some_and(|c| c.is_ascii_whitespace() || c == ',')
        {
            self.index += 1;
        }
        self.index < self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }
}

fn parse_f32(value: &str) -> Result<f32, SvgPathError> {
    value.parse().map_err(|_| invalid_number(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn parses_straight_path_commands() {
        let geometry = parse_path_data("M0,0 L10,0 H20 V10 l-5,5 h-5 v-5").unwrap();
        let SvgPathGeometry::Polyline(points) = geometry;
        assert_eq!(
            points,
            vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [20.0, 0.0],
                [20.0, 10.0],
                [15.0, 15.0],
                [10.0, 15.0],
                [10.0, 10.0],
            ]
        );
    }

    #[test]
    fn rejects_curves() {
        assert!(matches!(
            parse_path_data("M0,0 C1,1 2,2 3,3"),
            Err(SvgPathError::UnsupportedCommand('C'))
        ));
    }
}
