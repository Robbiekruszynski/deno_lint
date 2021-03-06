// Copyright 2020 the Deno authors. All rights reserved. MIT license.
use super::{Context, LintRule};
use swc_common::Span;
use swc_ecmascript::ast::Expr;
use swc_ecmascript::ast::Expr::{Assign, Bin, Paren};
use swc_ecmascript::ast::Module;
use swc_ecmascript::visit::{noop_visit_type, Node, VisitAll, VisitAllWith};

pub struct NoCondAssign;

impl LintRule for NoCondAssign {
  fn new() -> Box<Self> {
    Box::new(NoCondAssign)
  }

  fn tags(&self) -> &[&'static str] {
    &["recommended"]
  }

  fn code(&self) -> &'static str {
    "no-cond-assign"
  }

  fn lint_module(&self, context: &mut Context, module: &Module) {
    let mut visitor = NoCondAssignVisitor::new(context);
    module.visit_all_with(module, &mut visitor);
  }

  fn docs(&self) -> &'static str {
    r#"Disallows the use of the assignment operator, `=`, in conditional statements.

Use of the assignment operator within a conditional statement is often the result of mistyping the equality operator, `==`. If an assignment within a conditional statement is required then this rule allows it by wrapping the assignment in parentheses.

### Valid:
```typescript
var x;
if (x === 0) {
  var b = 1;
}
```
```typescript
function setHeight(someNode) {
  do {
    someNode.height = "100px";
  } while ((someNode = someNode.parentNode));
}
```

### Invalid:
```typescript
var x;
if (x = 0) {
  var b = 1;
}
```
```typescript
function setHeight(someNode) {
  do {
    someNode.height = "100px";
  } while (someNode = someNode.parentNode);
}
```"#
  }
}

struct NoCondAssignVisitor<'c> {
  context: &'c mut Context,
}

impl<'c> NoCondAssignVisitor<'c> {
  fn new(context: &'c mut Context) -> Self {
    Self { context }
  }

  fn add_diagnostic(&mut self, span: Span) {
    self.context.add_diagnostic(
      span,
      "no-cond-assign",
      "Expected a conditional expression and instead saw an assignment",
    );
  }

  fn check_condition(&mut self, condition: &Expr) {
    match condition {
      Assign(assign) => {
        self.add_diagnostic(assign.span);
      }
      Bin(bin) => {
        if bin.op == swc_ecmascript::ast::BinaryOp::LogicalOr {
          self.check_condition(&bin.left);
          self.check_condition(&bin.right);
        }
      }
      _ => {}
    }
  }
}

impl<'c> VisitAll for NoCondAssignVisitor<'c> {
  noop_visit_type!();

  fn visit_if_stmt(
    &mut self,
    if_stmt: &swc_ecmascript::ast::IfStmt,
    _parent: &dyn Node,
  ) {
    self.check_condition(&if_stmt.test);
  }

  fn visit_while_stmt(
    &mut self,
    while_stmt: &swc_ecmascript::ast::WhileStmt,
    _parent: &dyn Node,
  ) {
    self.check_condition(&while_stmt.test);
  }

  fn visit_do_while_stmt(
    &mut self,
    do_while_stmt: &swc_ecmascript::ast::DoWhileStmt,
    _parent: &dyn Node,
  ) {
    self.check_condition(&do_while_stmt.test);
  }

  fn visit_for_stmt(
    &mut self,
    for_stmt: &swc_ecmascript::ast::ForStmt,
    _parent: &dyn Node,
  ) {
    if let Some(for_test) = &for_stmt.test {
      self.check_condition(&for_test);
    }
  }

  fn visit_cond_expr(
    &mut self,
    cond_expr: &swc_ecmascript::ast::CondExpr,
    _parent: &dyn Node,
  ) {
    if let Paren(paren) = &*cond_expr.test {
      self.check_condition(&paren.expr);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_util::*;

  #[test]
  fn no_cond_assign_valid() {
    assert_lint_ok! {
      NoCondAssign,
      "if (x === 0) { };",
      "if ((x = y)) { }",
      "const x = 0; if (x == 0) { const b = 1; }",
      "const x = 5; while (x < 5) { x = x + 1; }",
      "while ((a = b));",
      "do {} while ((a = b));",
      "for (;(a = b););",
      "for (;;) {}",
      "if (someNode || (someNode = parentNode)) { }",
      "while (someNode || (someNode = parentNode)) { }",
      "do { } while (someNode || (someNode = parentNode));",
      "for (;someNode || (someNode = parentNode););",
      "if ((function(node) { return node = parentNode; })(someNode)) { }",
      "if ((node => node = parentNode)(someNode)) { }",
      "if (function(node) { return node = parentNode; }) { }",
      "const x; const b = (x === 0) ? 1 : 0;",
      "switch (foo) { case a = b: bar(); }",
    };
  }

  #[test]
  fn no_cond_assign_invalid() {
    assert_lint_err::<NoCondAssign>("if (x = 0) { }", 4);
    assert_lint_err::<NoCondAssign>("while (x = 0) { }", 7);
    assert_lint_err::<NoCondAssign>("do { } while (x = 0);", 14);
    assert_lint_err::<NoCondAssign>("for (let i = 0; i = 10; i++) { }", 16);
    assert_lint_err::<NoCondAssign>("const x; if (x = 0) { const b = 1; }", 13);
    assert_lint_err::<NoCondAssign>(
      "const x; while (x = 0) { const b = 1; }",
      16,
    );
    assert_lint_err::<NoCondAssign>(
      "const x = 0, y; do { y = x; } while (x = x + 1);",
      37,
    );
    assert_lint_err::<NoCondAssign>("let x; for(; x+=1 ;){};", 13);
    assert_lint_err::<NoCondAssign>("let x; if ((x) = (0));", 11);
    assert_lint_err::<NoCondAssign>("let x; let b = (x = 0) ? 1 : 0;", 16);
    assert_lint_err::<NoCondAssign>(
      "(((123.45)).abcd = 54321) ? foo : bar;",
      1,
    );

    // nested
    assert_lint_err::<NoCondAssign>("if (foo) { if (x = 0) {} }", 15);
    assert_lint_err::<NoCondAssign>("while (foo) { while (x = 0) {} }", 21);
    assert_lint_err::<NoCondAssign>(
      "do { do {} while (x = 0) } while (foo);",
      18,
    );
    assert_lint_err::<NoCondAssign>(
      "for (let i = 0; i < 10; i++) { for (; j+=1 ;) {} }",
      38,
    );
    assert_lint_err::<NoCondAssign>(
      "const val = foo ? (x = 0) ? 0 : 1 : 2;",
      19,
    );
  }
}
