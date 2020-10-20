import 'package:freezed_annotation/freezed_annotation.dart';

import 'ids.dart';
import 'type.dart';

part 'expressions.freezed.dart';
part 'expressions.g.dart';

@freezed
abstract class Expression implements _$Expression {
  const factory Expression.identifier(
    DeclarationLocalId id,
    Identifier identifier,
  ) = IdentifierExpression;
  const factory Expression.literal(DeclarationLocalId id, Literal literal) =
      LiteralExpression;
  const factory Expression.call(
    DeclarationLocalId id,
    Expression target,
    List<Expression> valueArguments,
  ) = CallExpression;
  const factory Expression.functionCall(
    DeclarationLocalId id,
    IdentifierExpression target,
    Map<String, Expression> valueArguments,
  ) = FunctionCallExpression;
  // ignore: non_constant_identifier_names
  const factory Expression.return_(
    DeclarationLocalId id,
    DeclarationLocalId scopeId,
    Expression expression,
  ) = ReturnExpression;

  factory Expression.fromJson(Map<String, dynamic> json) =>
      _$ExpressionFromJson(json);
  const Expression._();

  CandyType get type => when(
        identifier: (_, identifier) => identifier.type,
        literal: (_, literal) => literal.type,
        call: (_, __, ___) => null,
        functionCall: (_, target, __) {
          final functionType = target.type as FunctionCandyType;
          return functionType.returnType;
        },
        return_: (_, __, ___) => CandyType.never,
      );

  T accept<T>(ExpressionVisitor<T> visitor) => map(
        identifier: (e) => visitor.visitIdentifierExpression(e),
        literal: (e) => visitor.visitLiteralExpression(e),
        call: (e) => visitor.visitCallExpression(e),
        functionCall: (e) => visitor.visitFunctionCallExpression(e),
        return_: (e) => visitor.visitReturnExpression(e),
      );
}

@freezed
abstract class Identifier implements _$Identifier {
  // ignore: non_constant_identifier_names
  const factory Identifier.this_() = ThisIdentifier;
  // ignore: non_constant_identifier_names
  const factory Identifier.super_(CandyType type) = SuperIdentifier;
  const factory Identifier.it(CandyType type) = ItIdentifier;
  const factory Identifier.field(CandyType type) = FieldIdentifier;
  const factory Identifier.module(ModuleId id) = ModuleIdentifier;
  const factory Identifier.trait(DeclarationId id) = TraitIdentifier;
  // ignore: non_constant_identifier_names
  const factory Identifier.class_(DeclarationId id) = ClassIdentifier;

  const factory Identifier.parameter(
    DeclarationLocalId id,
    String name,
    CandyType type,
  ) = ParameterIdentifier;

  /// A property or function.
  const factory Identifier.property(
    Expression target,
    String name,
    CandyType type,
  ) = PropertyIdentifier;
  const factory Identifier.localProperty(
    DeclarationLocalId id,
    String name,
    CandyType type,
  ) = LocalPropertyIdentifier;

  factory Identifier.fromJson(Map<String, dynamic> json) =>
      _$IdentifierFromJson(json);
  const Identifier._();

  CandyType get type => when(
        this_: () => CandyType.this_(),
        super_: (type) => type,
        it: (type) => type,
        field: (type) => type,
        trait: (_) => CandyType.declaration,
        class_: (_) => CandyType.declaration,
        module: (_) => CandyType.declaration,
        parameter: (_, __, type) => type,
        property: (_, __, type) => type,
        localProperty: (_, __, type) => type,
      );

  T accept<T>(ExpressionVisitor<T> visitor) => map(
        this_: (i) => visitor.visitThisIdentifier(i),
        super_: (i) => visitor.visitSuperIdentifier(i),
        it: (i) => visitor.visitItIdentifier(i),
        field: (i) => visitor.visitFieldIdentifier(i),
        module: (i) => visitor.visitModuleIdentifier(i),
        trait: (i) => visitor.visitTraitIdentifier(i),
        class_: (i) => visitor.visitClassIdentifier(i),
        parameter: (i) => visitor.visitParameterIdentifier(i),
        property: (i) => visitor.visitPropertyIdentifier(i),
        localProperty: (i) => visitor.visitLocalPropertyIdentifier(i),
      );
}

@freezed
abstract class Literal implements _$Literal {
  // ignore: avoid_positional_boolean_parameters
  const factory Literal.boolean(bool value) = BoolLiteral;
  const factory Literal.integer(int value) = IntLiteral;
  const factory Literal.string(List<StringLiteralPart> parts) = StringLiteral;
  const factory Literal.lambda(
    List<Expression> expressions,
    FunctionCandyType type,
  ) = LambdaLiteral;

  factory Literal.fromJson(Map<String, dynamic> json) =>
      _$LiteralFromJson(json);
  const Literal._();

  CandyType get type => when(
        boolean: (_) => CandyType.bool,
        integer: (_) => CandyType.int,
        string: (_) => CandyType.string,
        lambda: (_, type) => type,
      );
}

@freezed
abstract class StringLiteralPart implements _$StringLiteralPart {
  const factory StringLiteralPart.literal(String value) =
      LiteralStringLiteralPart;
  const factory StringLiteralPart.interpolated(Expression value) =
      InterpolatedStringLiteralPart;

  factory StringLiteralPart.fromJson(Map<String, dynamic> json) =>
      _$StringLiteralPartFromJson(json);
  const StringLiteralPart._();
}

abstract class ExpressionVisitor<T> {
  const ExpressionVisitor();

  T visitIdentifierExpression(IdentifierExpression node);
  T visitLiteralExpression(LiteralExpression node);
  T visitCallExpression(CallExpression node);
  T visitFunctionCallExpression(FunctionCallExpression node);
  T visitReturnExpression(ReturnExpression node);

  T visitThisIdentifier(ThisIdentifier node);
  T visitSuperIdentifier(SuperIdentifier node);
  T visitItIdentifier(ItIdentifier node);
  T visitFieldIdentifier(FieldIdentifier node);
  T visitModuleIdentifier(ModuleIdentifier node);
  T visitTraitIdentifier(TraitIdentifier node);
  T visitClassIdentifier(ClassIdentifier node);
  T visitParameterIdentifier(ParameterIdentifier node);
  T visitPropertyIdentifier(PropertyIdentifier node);
  T visitLocalPropertyIdentifier(LocalPropertyIdentifier node);
}
