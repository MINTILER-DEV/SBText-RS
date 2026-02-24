use crate::ast::{EventScript, Expr, Project, Statement, Target};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub message: String,
}

impl Display for SemanticError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SemanticError {}

#[derive(Debug, Clone)]
struct ProcedureInfo {
    line: usize,
    params: Vec<String>,
}

pub fn analyze(project: &Project) -> Result<(), SemanticError> {
    if project.targets.is_empty() {
        return Err(SemanticError {
            message: "Project must define at least one target.".to_string(),
        });
    }
    let stage_count = project.targets.iter().filter(|t| t.is_stage).count();
    if stage_count > 1 {
        return Err(SemanticError {
            message: "Project can only define one stage.".to_string(),
        });
    }
    let mut names = HashSet::new();
    for target in &project.targets {
        let lowered = target.name.to_lowercase();
        if !names.insert(lowered) {
            return Err(SemanticError {
                message: format!("Duplicate target name '{}'.", target.name),
            });
        }
        analyze_target(target)?;
    }
    Ok(())
}

fn analyze_target(target: &Target) -> Result<(), SemanticError> {
    let mut variables: HashMap<String, usize> = HashMap::new();
    for decl in &target.variables {
        let lowered = decl.name.to_lowercase();
        if variables.contains_key(&lowered) {
            return Err(SemanticError {
                message: format!(
                    "Duplicate variable '{}' in target '{}' at line {}, column {}.",
                    decl.name, target.name, decl.pos.line, decl.pos.column
                ),
            });
        }
        variables.insert(lowered, decl.pos.line);
    }

    let mut lists: HashMap<String, usize> = HashMap::new();
    for decl in &target.lists {
        let lowered = decl.name.to_lowercase();
        if lists.contains_key(&lowered) {
            return Err(SemanticError {
                message: format!(
                    "Duplicate list '{}' in target '{}' at line {}, column {}.",
                    decl.name, target.name, decl.pos.line, decl.pos.column
                ),
            });
        }
        lists.insert(lowered, decl.pos.line);
    }

    let mut procedures: HashMap<String, ProcedureInfo> = HashMap::new();
    for procedure in &target.procedures {
        let lowered = procedure.name.to_lowercase();
        if let Some(prev) = procedures.get(&lowered) {
            return Err(SemanticError {
                message: format!(
                    "Procedure '{}' is already defined at line {} in target '{}'.",
                    procedure.name, prev.line, target.name
                ),
            });
        }
        let mut param_names = HashSet::new();
        for p in &procedure.params {
            if !param_names.insert(p.to_lowercase()) {
                return Err(SemanticError {
                    message: format!(
                        "Procedure '{}' has duplicate parameter names at line {}, column {}.",
                        procedure.name, procedure.pos.line, procedure.pos.column
                    ),
                });
            }
        }
        procedures.insert(
            lowered,
            ProcedureInfo {
                line: procedure.pos.line,
                params: procedure.params.clone(),
            },
        );
    }

    for procedure in &target.procedures {
        let param_scope = procedure
            .params
            .iter()
            .map(|p| p.to_lowercase())
            .collect::<HashSet<_>>();
        analyze_statements(
            target,
            &procedure.body,
            &variables,
            &lists,
            &procedures,
            &param_scope,
            &format!("procedure '{}'", procedure.name),
        )?;
    }

    for script in &target.scripts {
        analyze_event_script(target, script, &variables, &lists, &procedures)?;
    }

    Ok(())
}

fn analyze_event_script(
    target: &Target,
    script: &EventScript,
    variables: &HashMap<String, usize>,
    lists: &HashMap<String, usize>,
    procedures: &HashMap<String, ProcedureInfo>,
) -> Result<(), SemanticError> {
    analyze_statements(
        target,
        &script.body,
        variables,
        lists,
        procedures,
        &HashSet::new(),
        "event script",
    )
}

fn analyze_statements(
    target: &Target,
    statements: &[Statement],
    variables: &HashMap<String, usize>,
    lists: &HashMap<String, usize>,
    procedures: &HashMap<String, ProcedureInfo>,
    param_scope: &HashSet<String>,
    scope_name: &str,
) -> Result<(), SemanticError> {
    for stmt in statements {
        match stmt {
            Statement::Broadcast { message, pos } => {
                if message.is_empty() {
                    return Err(SemanticError {
                        message: format!(
                            "Broadcast message cannot be empty at line {}, column {} in target '{}'.",
                            pos.line, pos.column, target.name
                        ),
                    });
                }
            }
            Statement::SetVar { var_name, value, pos } => {
                ensure_variable_exists(target, var_name, variables, param_scope, pos.line, pos.column)?;
                analyze_expr(target, value, variables, lists, param_scope)?;
            }
            Statement::ChangeVar { var_name, delta, pos } => {
                ensure_variable_exists(target, var_name, variables, param_scope, pos.line, pos.column)?;
                analyze_expr(target, delta, variables, lists, param_scope)?;
            }
            Statement::Move { steps, .. } => analyze_expr(target, steps, variables, lists, param_scope)?,
            Statement::Say { message, .. } => analyze_expr(target, message, variables, lists, param_scope)?,
            Statement::Think { message, .. } => analyze_expr(target, message, variables, lists, param_scope)?,
            Statement::Wait { duration, .. } => analyze_expr(target, duration, variables, lists, param_scope)?,
            Statement::Repeat { times, body, .. } => {
                analyze_expr(target, times, variables, lists, param_scope)?;
                analyze_statements(target, body, variables, lists, procedures, param_scope, scope_name)?;
            }
            Statement::Forever { body, .. } => {
                analyze_statements(target, body, variables, lists, procedures, param_scope, scope_name)?;
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                analyze_expr(target, condition, variables, lists, param_scope)?;
                analyze_statements(target, then_body, variables, lists, procedures, param_scope, scope_name)?;
                analyze_statements(target, else_body, variables, lists, procedures, param_scope, scope_name)?;
            }
            Statement::ProcedureCall { name, args, pos } => {
                let Some(proc_info) = procedures.get(&name.to_lowercase()) else {
                    return Err(SemanticError {
                        message: format!(
                            "Unknown procedure '{}' at line {}, column {} in target '{}'.",
                            name, pos.line, pos.column, target.name
                        ),
                    });
                };
                if pos.line < proc_info.line {
                    return Err(SemanticError {
                        message: format!(
                            "Procedure '{}' is used before it is defined (call line {}, definition line {}) in target '{}'.",
                            name, pos.line, proc_info.line, target.name
                        ),
                    });
                }
                if args.len() != proc_info.params.len() {
                    return Err(SemanticError {
                        message: format!(
                            "Procedure '{}' expects {} argument(s), got {} at line {}, column {} in {}.",
                            name,
                            proc_info.params.len(),
                            args.len(),
                            pos.line,
                            pos.column,
                            scope_name
                        ),
                    });
                }
                for arg in args {
                    analyze_expr(target, arg, variables, lists, param_scope)?;
                }
            }
            Statement::TurnRight { degrees, .. } => analyze_expr(target, degrees, variables, lists, param_scope)?,
            Statement::TurnLeft { degrees, .. } => analyze_expr(target, degrees, variables, lists, param_scope)?,
            Statement::GoToXY { x, y, .. } => {
                analyze_expr(target, x, variables, lists, param_scope)?;
                analyze_expr(target, y, variables, lists, param_scope)?;
            }
            Statement::ChangeXBy { value, .. }
            | Statement::SetX { value, .. }
            | Statement::ChangeYBy { value, .. }
            | Statement::SetY { value, .. }
            | Statement::ChangeSizeBy { value, .. }
            | Statement::SetSizeTo { value, .. } => analyze_expr(target, value, variables, lists, param_scope)?,
            Statement::PointInDirection { direction, .. } => {
                analyze_expr(target, direction, variables, lists, param_scope)?
            }
            Statement::IfOnEdgeBounce { .. }
            | Statement::Show { .. }
            | Statement::Hide { .. }
            | Statement::NextCostume { .. }
            | Statement::NextBackdrop { .. }
            | Statement::ResetTimer { .. } => {}
            Statement::Stop { option, .. } => analyze_expr(target, option, variables, lists, param_scope)?,
            Statement::Ask { question, .. } => analyze_expr(target, question, variables, lists, param_scope)?,
            Statement::AddToList {
                list_name,
                item,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
                analyze_expr(target, item, variables, lists, param_scope)?;
            }
            Statement::DeleteOfList {
                list_name,
                index,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
                analyze_expr(target, index, variables, lists, param_scope)?;
            }
            Statement::DeleteAllOfList { list_name, pos } => {
                ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
            }
            Statement::InsertAtList {
                list_name,
                item,
                index,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
                analyze_expr(target, item, variables, lists, param_scope)?;
                analyze_expr(target, index, variables, lists, param_scope)?;
            }
            Statement::ReplaceItemOfList {
                list_name,
                index,
                item,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
                analyze_expr(target, index, variables, lists, param_scope)?;
                analyze_expr(target, item, variables, lists, param_scope)?;
            }
        }
    }
    Ok(())
}

fn analyze_expr(
    target: &Target,
    expr: &Expr,
    variables: &HashMap<String, usize>,
    lists: &HashMap<String, usize>,
    param_scope: &HashSet<String>,
) -> Result<(), SemanticError> {
    match expr {
        Expr::Var { name, pos } => {
            let lowered = name.to_lowercase();
            if param_scope.contains(&lowered) {
                return Ok(());
            }
            if !variables.contains_key(&lowered) {
                return Err(SemanticError {
                    message: format!(
                        "Unknown variable '{}' at line {}, column {} in target '{}'.",
                        name, pos.line, pos.column, target.name
                    ),
                });
            }
            Ok(())
        }
        Expr::Unary { operand, .. } => analyze_expr(target, operand, variables, lists, param_scope),
        Expr::Binary { left, right, .. } => {
            analyze_expr(target, left, variables, lists, param_scope)?;
            analyze_expr(target, right, variables, lists, param_scope)
        }
        Expr::PickRandom { start, end, .. } => {
            analyze_expr(target, start, variables, lists, param_scope)?;
            analyze_expr(target, end, variables, lists, param_scope)
        }
        Expr::ListItem {
            list_name,
            index,
            pos,
        } => {
            ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
            analyze_expr(target, index, variables, lists, param_scope)
        }
        Expr::ListLength { list_name, pos } => {
            ensure_list_exists(target, list_name, lists, pos.line, pos.column)
        }
        Expr::ListContains {
            list_name,
            item,
            pos,
        } => {
            ensure_list_exists(target, list_name, lists, pos.line, pos.column)?;
            analyze_expr(target, item, variables, lists, param_scope)
        }
        Expr::KeyPressed { key, .. } => analyze_expr(target, key, variables, lists, param_scope),
        Expr::BuiltinReporter { .. } | Expr::Number { .. } | Expr::String { .. } => Ok(()),
    }
}

fn ensure_variable_exists(
    target: &Target,
    name: &str,
    variables: &HashMap<String, usize>,
    param_scope: &HashSet<String>,
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    let lowered = name.to_lowercase();
    if param_scope.contains(&lowered) {
        return Err(SemanticError {
            message: format!(
                "Variable field '{}' refers to a procedure parameter at line {}, column {}; Scratch variable blocks must target declared variables.",
                name, line, column
            ),
        });
    }
    if !variables.contains_key(&lowered) {
        return Err(SemanticError {
            message: format!(
                "Unknown variable '{}' at line {}, column {} in target '{}'.",
                name, line, column, target.name
            ),
        });
    }
    Ok(())
}

fn ensure_list_exists(
    target: &Target,
    name: &str,
    lists: &HashMap<String, usize>,
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    if !lists.contains_key(&name.to_lowercase()) {
        return Err(SemanticError {
            message: format!(
                "Unknown list '{}' at line {}, column {} in target '{}'.",
                name, line, column, target.name
            ),
        });
    }
    Ok(())
}
