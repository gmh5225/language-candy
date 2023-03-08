#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CstError {
    BinaryBarMissesRight,
    CurlyBraceNotClosed,
    IdentifierContainsNonAlphanumericAscii,
    IntContainsNonDigits,
    ListItemMissesValue,
    ListNotClosed,
    MatchCaseMissesArrow,
    MatchCaseMissesBody,
    MatchMissesCases,
    OpeningParenthesisMissesExpression,
    OrPatternMissesRight,
    ParenthesisNotClosed,
    StructFieldMissesColon,
    StructFieldMissesKey,
    StructFieldMissesValue,
    StructNotClosed,
    SymbolContainsNonAlphanumericAscii,
    TextInterpolationMissesExpression,
    TextInterpolationNotClosed,
    TextNotClosed,
    TextNotSufficientlyIndented,
    TooMuchWhitespace,
    UnexpectedCharacters,
    UnparsedRest,
    WeirdWhitespace,
    WeirdWhitespaceInIndentation,
}
