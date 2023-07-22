use derive_more::{Add, From, Sub};
use extension_trait::extension_trait;
use std::{
    fmt::{self, Display, Formatter},
    iter::Sum,
    ops::{Add, AddAssign},
};

#[derive(Clone, Copy, Debug, Default, From)]
pub struct Indentation(usize);
impl Indentation {
    pub fn width(self) -> SinglelineWidth {
        SinglelineWidth::from(self.0 * 2)
    }
    pub fn is_indented(self) -> bool {
        self.0 > 0
    }

    pub fn with_indent(self) -> Self {
        Self(self.0 + 1)
    }
    pub fn with_dedent(self) -> Self {
        Self(self.0 - 1)
    }
}
impl Display for Indentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", "  ".repeat(self.0))?;
        Ok(())
    }
}

// SinglelineWidth

#[derive(Add, Clone, Copy, Debug, Default, Eq, From, Ord, PartialEq, PartialOrd, Sub)]
pub struct SinglelineWidth(usize);
impl SinglelineWidth {
    pub const SPACE: SinglelineWidth = SinglelineWidth(1);
    pub const PERCENT: SinglelineWidth = SinglelineWidth(1);

    pub const fn new_const(width: usize) -> Self {
        SinglelineWidth(width)
    }

    pub fn is_empty(self) -> bool {
        self == 0.into()
    }
}
impl Add<Width> for SinglelineWidth {
    type Output = Width;
    fn add(self, rhs: Width) -> Self::Output {
        Width::from(self) + rhs
    }
}

// Width

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Width {
    Singleline(SinglelineWidth),
    Multiline {
        /// Only [Some] if the expression can be used as a trailing multiline expression, e.g., a
        /// trailing function.
        first_line_width: Option<SinglelineWidth>,
        last_line_width: Option<SinglelineWidth>,
    },
}
impl Width {
    pub const MAX: SinglelineWidth = SinglelineWidth::new_const(100);
    pub const NEWLINE: Width = Width::Multiline {
        first_line_width: Some(SinglelineWidth::new_const(0)),
        last_line_width: Some(SinglelineWidth::new_const(0)),
    };

    pub fn multiline(
        first_line_width: impl Into<Option<SinglelineWidth>>,
        last_line_width: impl Into<Option<SinglelineWidth>>,
    ) -> Self {
        Width::Multiline {
            first_line_width: first_line_width.into(),
            last_line_width: last_line_width.into(),
        }
    }

    pub fn from_width_and_max(width: SinglelineWidth, max_width: SinglelineWidth) -> Self {
        if width > max_width {
            Width::Multiline {
                first_line_width: None,
                last_line_width: None,
            }
        } else {
            Width::Singleline(width)
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Width::Singleline(width) => width.is_empty(),
            Width::Multiline { .. } => false,
        }
    }
    pub fn is_singleline(&self) -> bool {
        match self {
            Width::Singleline(_) => true,
            Width::Multiline { .. } => false,
        }
    }
    pub fn first_line_width(&self) -> Option<SinglelineWidth> {
        match self {
            Width::Singleline(width) => Some(*width),
            Width::Multiline {
                first_line_width, ..
            } => *first_line_width,
        }
    }
    pub fn without_first_line_width(&self) -> Width {
        match self {
            Width::Singleline(width) => Width::Singleline(*width),
            Width::Multiline {
                last_line_width, ..
            } => Width::Multiline {
                first_line_width: None,
                last_line_width: *last_line_width,
            },
        }
    }

    pub fn fits(&self, indentation: Indentation) -> bool {
        self.fits_in(Width::MAX - indentation.width())
    }
    pub fn fits_in(&self, max_width: SinglelineWidth) -> bool {
        match self {
            Width::Singleline(width) => width <= &max_width,
            Width::Multiline { .. } => false,
        }
    }
    pub fn last_line_fits(&self, indentation: Indentation, extra_width: impl Into<Width>) -> bool {
        let Width::Singleline(extra_width) = extra_width.into() else {
            return false;
        };
        match self {
            Width::Singleline(self_width) => {
                indentation.width() + *self_width + extra_width <= Width::MAX
            }
            Width::Multiline {
                last_line_width, ..
            } => last_line_width.unwrap() + extra_width <= Width::MAX,
        }
    }
}
impl Default for Width {
    fn default() -> Self {
        Width::Singleline(SinglelineWidth::default())
    }
}
impl From<usize> for Width {
    fn from(width: usize) -> Self {
        SinglelineWidth::from(width).into()
    }
}
impl From<SinglelineWidth> for Width {
    fn from(width: SinglelineWidth) -> Self {
        Self::from_width_and_max(width, Width::MAX)
    }
}

impl Add<Width> for Width {
    type Output = Width;

    fn add(self, rhs: Width) -> Self::Output {
        fn add_singleline(
            lhs: impl Into<Option<SinglelineWidth>>,
            rhs: impl Into<Option<SinglelineWidth>>,
        ) -> Option<SinglelineWidth> {
            let (Some(lhs), Some(rhs)) = (lhs.into(), rhs.into()) else {
                return None;
            };
            let sum = lhs + rhs;
            if sum <= Width::MAX {
                Some(sum)
            } else {
                None
            }
        }

        match (self, rhs) {
            (Width::Singleline(lhs), Width::Singleline(rhs)) => (lhs + rhs).into(),
            (
                Width::Singleline(lhs),
                Width::Multiline {
                    first_line_width,
                    last_line_width,
                },
            ) => Width::multiline(add_singleline(lhs, first_line_width), last_line_width),
            (
                Width::Multiline {
                    first_line_width,
                    last_line_width,
                },
                Width::Singleline(rhs),
            ) => Width::multiline(first_line_width, add_singleline(last_line_width, rhs)),
            (
                Width::Multiline {
                    first_line_width, ..
                },
                Width::Multiline {
                    last_line_width, ..
                },
            ) => Width::multiline(first_line_width, last_line_width),
        }
    }
}
impl Add<SinglelineWidth> for Width {
    type Output = Width;

    fn add(self, rhs: SinglelineWidth) -> Self::Output {
        self + Width::from(rhs)
    }
}

impl AddAssign<Width> for Width {
    fn add_assign(&mut self, rhs: Width) {
        *self = *self + rhs;
    }
}
impl AddAssign<SinglelineWidth> for Width {
    fn add_assign(&mut self, rhs: SinglelineWidth) {
        *self += Width::from(rhs);
    }
}

impl Sum for Width {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Width::default(), |acc, width| acc + width)
    }
}

#[extension_trait]
pub impl StringWidth for str {
    fn width(&self) -> Width {
        if let Some(first_index) = self.find('\n') {
            let last_index = self.rfind('\n').unwrap();
            Width::multiline(
                SinglelineWidth::from(unicode_width::UnicodeWidthStr::width(&self[..first_index])),
                SinglelineWidth::from(unicode_width::UnicodeWidthStr::width(
                    &self[last_index + 1..],
                )),
            )
        } else {
            Width::Singleline(SinglelineWidth::from(
                unicode_width::UnicodeWidthStr::width(self),
            ))
        }
    }
}
