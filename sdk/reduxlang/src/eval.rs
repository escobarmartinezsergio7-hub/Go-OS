use std::collections::HashMap;

use crate::ast::{Expr, Op, Statement};

#[derive(Default)]
pub struct Env {
    values: HashMap<String, i64>,
}

impl Env {
    pub fn eval_program(&mut self, program: &[Statement]) -> Result<Option<i64>, String> {
        let mut last = None;
        for stmt in program {
            last = Some(self.eval_statement(stmt)?);
        }
        Ok(last)
    }

    fn eval_statement(&mut self, stmt: &Statement) -> Result<i64, String> {
        match stmt {
            Statement::Let(name, expr) => {
                let v = self.eval_expr(expr)?;
                self.values.insert(name.clone(), v);
                Ok(v)
            }
            Statement::Expr(expr) => self.eval_expr(expr),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<i64, String> {
        match expr {
            Expr::Int(v) => Ok(*v),
            Expr::Ident(name) => self
                .values
                .get(name)
                .copied()
                .ok_or_else(|| format!("undefined variable: {name}")),
            Expr::Infix(left, op, right) => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                match op {
                    Op::Add => Ok(l + r),
                    Op::Sub => Ok(l - r),
                    Op::Mul => Ok(l * r),
                    Op::Div => {
                        if r == 0 {
                            Err("division by zero".to_string())
                        } else {
                            Ok(l / r)
                        }
                    }
                }
            }
        }
    }

    pub fn dump(&self) -> Vec<(String, i64)> {
        let mut vars: Vec<(String, i64)> = self.values.iter().map(|(k, v)| (k.clone(), *v)).collect();
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        vars
    }
}
