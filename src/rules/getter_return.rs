// Copyright 2020 the Deno authors. All rights reserved. MIT license.
use super::Context;
use super::LintRule;
use crate::swc_util::Key;
use std::collections::BTreeMap;
use std::mem;
use swc_common::{Span, Spanned};
use swc_ecmascript::ast::{
  BlockStmtOrExpr, CallExpr, ClassMethod, Expr, ExprOrSuper, GetterProp,
  MethodKind, PrivateMethod, Prop, PropName, PropOrSpread, ReturnStmt,
};
use swc_ecmascript::visit::noop_visit_type;
use swc_ecmascript::visit::Node;
use swc_ecmascript::visit::Visit;
use swc_ecmascript::visit::VisitWith;

pub struct GetterReturn;

impl LintRule for GetterReturn {
  fn new() -> Box<Self> {
    Box::new(GetterReturn)
  }

  fn tags(&self) -> &[&'static str] {
    &["recommended"]
  }

  fn code(&self) -> &'static str {
    "getter-return"
  }

  fn lint_module(
    &self,
    context: &mut Context,
    module: &swc_ecmascript::ast::Module,
  ) {
    let mut visitor = GetterReturnVisitor::new(context);
    visitor.visit_module(module, module);
    visitor.report();
  }

  fn docs(&self) -> &'static str {
    r#"Requires all property getter functions to return a value

Getter functions return the value of a property.  If the function returns no
value then this contract is broken.
    
### Valid:
```typescript
let foo = { 
  get bar() { 
    return true; 
  }
};

class Person { 
  get name() { 
    return "alice"; 
  }
}
```

### Invalid:
```typescript
let foo = { 
  get bar() {}
};

class Person { 
  get name() {}
}
```"#
  }
}

struct GetterReturnVisitor<'c> {
  context: &'c mut Context,
  errors: BTreeMap<Span, String>,
  /// If this visitor is currently in a getter, its name is stored.
  getter_name: Option<String>,
  // `true` if a getter contains as least one return statement.
  has_return: bool,
}

impl<'c> GetterReturnVisitor<'c> {
  fn new(context: &'c mut Context) -> Self {
    Self {
      context,
      errors: BTreeMap::new(),
      getter_name: None,
      has_return: false,
    }
  }

  fn report(&mut self) {
    for (span, msg) in &self.errors {
      self.context.add_diagnostic_with_hint(
        *span,
        "getter-return",
        msg,
        "Return a value from the getter function",
      );
    }
  }

  fn report_expected(&mut self, span: Span) {
    self.errors.insert(
      span,
      format!(
        "Expected to return a value in {}.",
        self
          .getter_name
          .clone()
          .expect("the name of getter is not set")
      ),
    );
  }

  fn report_always_expected(&mut self, span: Span) {
    self.errors.insert(
      span,
      format!(
        "Expected {} to always return a value.",
        self
          .getter_name
          .clone()
          .expect("the name of getter is not set")
      ),
    );
  }

  fn check_getter(&mut self, getter_body_span: Span, getter_span: Span) {
    if self.getter_name.is_none() {
      return;
    }

    if self
      .context
      .control_flow
      .meta(getter_body_span.lo)
      .unwrap()
      .continues_execution()
    {
      if self.has_return {
        self.report_always_expected(getter_span);
      } else {
        self.report_expected(getter_span);
      }
    }
  }

  fn set_getter_name<T: Key>(&mut self, name: &T) {
    self.getter_name =
      Some(name.get_key().unwrap_or_else(|| "[GETTER]".to_string()));
  }

  fn visit_getter<F>(&mut self, op: F)
  where
    F: FnOnce(&mut Self),
  {
    let prev_name = mem::take(&mut self.getter_name);
    let prev_has_return = self.has_return;
    op(self);
    self.getter_name = prev_name;
    self.has_return = prev_has_return;
  }
}

impl<'c> Visit for GetterReturnVisitor<'c> {
  noop_visit_type!();

  fn visit_class_method(&mut self, class_method: &ClassMethod, _: &dyn Node) {
    self.visit_getter(|a| {
      if class_method.kind == MethodKind::Getter {
        a.set_getter_name(&class_method.key);
      }
      class_method.visit_children_with(a);

      if let Some(body) = &class_method.function.body {
        a.check_getter(body.span, class_method.span);
      }
    });
  }

  fn visit_private_method(
    &mut self,
    private_method: &PrivateMethod,
    _: &dyn Node,
  ) {
    self.visit_getter(|a| {
      if private_method.kind == MethodKind::Getter {
        a.set_getter_name(&private_method.key);
      }
      private_method.visit_children_with(a);

      if let Some(body) = &private_method.function.body {
        a.check_getter(body.span, private_method.span);
      }
    });
  }

  fn visit_getter_prop(&mut self, getter_prop: &GetterProp, _: &dyn Node) {
    self.visit_getter(|a| {
      a.set_getter_name(&getter_prop.key);
      getter_prop.visit_children_with(a);

      if let Some(body) = &getter_prop.body {
        a.check_getter(body.span, getter_prop.span);
      }
    });
  }

  fn visit_call_expr(&mut self, call_expr: &CallExpr, _parent: &dyn Node) {
    call_expr.visit_children_with(self);

    if call_expr.args.len() != 3 {
      return;
    }
    if let ExprOrSuper::Expr(callee_expr) = &call_expr.callee {
      if let Expr::Member(member) = &**callee_expr {
        if let ExprOrSuper::Expr(member_obj) = &member.obj {
          if let Expr::Ident(ident) = &**member_obj {
            if ident.sym != *"Object" {
              return;
            }
          }
        }
        if let Expr::Ident(ident) = &*member.prop {
          if ident.sym != *"defineProperty" {
            return;
          }
        }
      }
    }
    if let Expr::Object(obj_expr) = &*call_expr.args[2].expr {
      for prop in obj_expr.props.iter() {
        if let PropOrSpread::Prop(prop_expr) = prop {
          if let Prop::KeyValue(kv_prop) = &**prop_expr {
            if let PropName::Ident(ident) = &kv_prop.key {
              if ident.sym != *"get" {
                return;
              }

              self.visit_getter(|a| {
                a.set_getter_name(&kv_prop.key);

                if let Expr::Fn(fn_expr) = &*kv_prop.value {
                  if let Some(body) = &fn_expr.function.body {
                    body.visit_children_with(a);
                    a.check_getter(body.span, prop.span());
                  }
                } else if let Expr::Arrow(arrow_expr) = &*kv_prop.value {
                  if let BlockStmtOrExpr::BlockStmt(block_stmt) =
                    &arrow_expr.body
                  {
                    block_stmt.visit_children_with(a);
                    a.check_getter(block_stmt.span, prop.span());
                  }
                }
              });
            }
          } else if let Prop::Method(method_prop) = &**prop_expr {
            if let PropName::Ident(ident) = &method_prop.key {
              if ident.sym != *"get" {
                return;
              }

              self.visit_getter(|a| {
                a.set_getter_name(&method_prop.key);

                if let Some(body) = &method_prop.function.body {
                  body.visit_children_with(a);
                  a.check_getter(body.span, prop.span());
                }
              });
            }
          }
        }
      }
    }
  }

  fn visit_return_stmt(&mut self, return_stmt: &ReturnStmt, _: &dyn Node) {
    if self.getter_name.is_some() {
      self.has_return = true;
      if return_stmt.arg.is_none() {
        self.report_expected(return_stmt.span);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_util::*;

  // Some tests are derived from
  // https://github.com/eslint/eslint/blob/v7.9.0/tests/lib/rules/getter-return.js
  // MIT Licensed.

  #[test]
  fn getter_return_valid() {
    assert_lint_ok! {
      GetterReturn,
      "let foo = { get bar() { return true; } };",
      "class Foo { get bar() { return true; } }",
      "class Foo { bar() {} }",
      "class Foo { get bar() { if (baz) { return true; } else { return false; } } }",
      "class Foo { get() { return true; } }",
      r#"Object.defineProperty(foo, "bar", { get: function () { return true; } });"#,
      r#"Object.defineProperty(foo, "bar",
         { get: function () { ~function() { return true; }(); return true; } });"#,
      r#"Object.defineProperties(foo,
         { bar: { get: function() { return true; } } });"#,
      r#"Object.defineProperties(foo,
         { bar: { get: function () { ~function() { return true; }(); return true; } } });"#,
      "let get = function() {};",
      "let get = function() { return true; };",
      "let foo = { bar() {} };",
      "let foo = { bar() { return true; } };",
      "let foo = { bar: function() {} };",
      "let foo = { bar: function() { return; } };",
      "let foo = { bar: function() { return true; } };",
      "let foo = { get: function() {} };",
      "let foo = { get: () => {} };",
      r#"
const foo = {
  get getter() {
    const bar = {
      get getter() {
        return true;
      }
    };
    return 42;
  }
};
"#,
      r#"
class Foo {
  get foo() {
    class Bar {
      get bar() {
        return true;
      }
    };
    return 42;
  }
}
"#,
      r#"
Object.defineProperty(foo, 'bar', {
  get: function() {
    Object.defineProperty(x, 'y', {
      get: function() {
        return true;
      }
    });
    return 42;
  }
});
      "#,
      // https://github.com/denoland/deno_lint/issues/348
      r#"
const obj = {
  get root() {
    let primary = this;
    while (true) {
      if (primary.parent !== undefined) {
          primary = primary.parent;
      } else {
          return primary;
      }
    }
  }
};
      "#,
    };
  }

  #[test]
  fn getter_return_invalid() {
    // object getter
    assert_lint_err::<GetterReturn>("const foo = { get getter() {} };", 14);
    assert_lint_err::<GetterReturn>(
      "const foo = { get bar() { ~function() { return true; } } };",
      14,
    );
    assert_lint_err::<GetterReturn>(
      "const foo = { get bar() { if (baz) { return true; } } };",
      14,
    );
    assert_lint_err::<GetterReturn>(
      "const foo = { get bar() { return; } };",
      26,
    );
    // class getter
    assert_lint_err::<GetterReturn>("class Foo { get bar() {} }", 12);
    assert_lint_err::<GetterReturn>(
      "const foo = class { static get bar() {} }",
      20,
    );
    assert_lint_err::<GetterReturn>(
      "class Foo { get bar(){ if (baz) { return true; } } }",
      12,
    );
    assert_lint_err::<GetterReturn>(
      "class Foo { get bar(){ ~function () { return true; }() } }",
      12,
    );
    // Object.defineProperty
    assert_lint_err::<GetterReturn>(
      "Object.defineProperty(foo, 'bar', { get: function(){} });",
      36,
    );
    assert_lint_err::<GetterReturn>(
      "Object.defineProperty(foo, 'bar', { get: function getfoo(){} });",
      36,
    );
    assert_lint_err::<GetterReturn>(
      "Object.defineProperty(foo, 'bar', { get(){} });",
      36,
    );
    assert_lint_err::<GetterReturn>(
      "Object.defineProperty(foo, 'bar', { get: () => {} });",
      36,
    );
    assert_lint_err::<GetterReturn>(
      r#"Object.defineProperty(foo, "bar", { get: function() { if(bar) { return true; } } });"#,
      36,
    );
    assert_lint_err::<GetterReturn>(
      r#"Object.defineProperty(foo, "bar", { get: function(){ ~function() { return true; }() } });"#,
      36,
    );
    // optional chaining
    assert_lint_err::<GetterReturn>(
      r#"Object?.defineProperty(foo, 'bar', { get: function(){} });"#,
      37,
    );
    assert_lint_err::<GetterReturn>(
      r#"(Object?.defineProperty)(foo, 'bar', { get: function(){} });"#,
      39,
    );
    // nested
    assert_lint_err_on_line::<GetterReturn>(
      r#"
const foo = {
  get getter() {
    const bar = {
      get getter() {}
    };
    return 42;
  }
};
      "#,
      5,
      6,
    );
    assert_lint_err_on_line::<GetterReturn>(
      r#"
class Foo {
  get foo() {
    class Bar {
      get bar() {}
    };
    return 42;
  }
}
      "#,
      5,
      6,
    );
    assert_lint_err_on_line::<GetterReturn>(
      r#"
Object.defineProperty(foo, 'bar', {
  get: function() {
    Object.defineProperty(x, 'y', {
      get: function() {}
    });
    return 42;
  }
});
      "#,
      5,
      6,
    );
    // other
    assert_lint_err_n::<GetterReturn>(
      "class b { get getterA() {} private get getterB() {} }",
      vec![10, 27],
    );
  }
}
