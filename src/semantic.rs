use crate::ast::{EventScript, Expr, Project, Statement, Target};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SemanticOptions {
    pub allow_unknown_procedures: bool,
}

#[derive(Debug, Clone)]
pub struct SemanticWarning {
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticReport {
    pub warnings: Vec<SemanticWarning>,
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

#[derive(Debug, Clone)]
struct TargetInfo {
    name: String,
    variables: HashSet<String>,
    lists: HashSet<String>,
    procedures: HashMap<String, usize>,
}

pub fn analyze(project: &Project) -> Result<(), SemanticError> {
    analyze_with_options(project, SemanticOptions::default()).map(|_| ())
}

pub fn analyze_with_options(
    project: &Project,
    options: SemanticOptions,
) -> Result<SemanticReport, SemanticError> {
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
    }

    let mut target_infos: HashMap<String, TargetInfo> = HashMap::new();
    for target in &project.targets {
        let mut vars = HashSet::new();
        for decl in &target.variables {
            vars.insert(decl.name.to_lowercase());
        }
        let mut lists = HashSet::new();
        for decl in &target.lists {
            lists.insert(decl.name.to_lowercase());
        }
        let mut procs = HashMap::new();
        for procedure in &target.procedures {
            procs.insert(procedure.name.to_lowercase(), procedure.params.len());
        }
        target_infos.insert(
            target.name.to_lowercase(),
            TargetInfo {
                name: target.name.clone(),
                variables: vars,
                lists,
                procedures: procs,
            },
        );
    }

    let mut warnings = Vec::new();
    for target in &project.targets {
        analyze_target(target, &target_infos, options, &mut warnings)?;
    }
    Ok(SemanticReport { warnings })
}

fn analyze_target(
    target: &Target,
    target_infos: &HashMap<String, TargetInfo>,
    options: SemanticOptions,
    warnings: &mut Vec<SemanticWarning>,
) -> Result<(), SemanticError> {
    let mut variables: HashMap<String, usize> = HashMap::new();
    for decl in &target.variables {
        let lowered = decl.name.to_lowercase();
        if variables.contains_key(&lowered) {
            continue;
        }
        variables.insert(lowered, decl.pos.line);
    }

    let mut lists: HashMap<String, usize> = HashMap::new();
    for decl in &target.lists {
        let lowered = decl.name.to_lowercase();
        if lists.contains_key(&lowered) {
            continue;
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
            target_infos,
            &param_scope,
            &format!("procedure '{}'", procedure.name),
            options,
            warnings,
        )?;
    }

    for script in &target.scripts {
        analyze_event_script(
            target,
            script,
            &variables,
            &lists,
            &procedures,
            target_infos,
            options,
            warnings,
        )?;
    }

    Ok(())
}

fn analyze_event_script(
    target: &Target,
    script: &EventScript,
    variables: &HashMap<String, usize>,
    lists: &HashMap<String, usize>,
    procedures: &HashMap<String, ProcedureInfo>,
    target_infos: &HashMap<String, TargetInfo>,
    options: SemanticOptions,
    warnings: &mut Vec<SemanticWarning>,
) -> Result<(), SemanticError> {
    analyze_statements(
        target,
        &script.body,
        variables,
        lists,
        procedures,
        target_infos,
        &HashSet::new(),
        "event script",
        options,
        warnings,
    )
}

fn analyze_statements(
    target: &Target,
    statements: &[Statement],
    variables: &HashMap<String, usize>,
    lists: &HashMap<String, usize>,
    procedures: &HashMap<String, ProcedureInfo>,
    target_infos: &HashMap<String, TargetInfo>,
    param_scope: &HashSet<String>,
    scope_name: &str,
    options: SemanticOptions,
    warnings: &mut Vec<SemanticWarning>,
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
            Statement::BroadcastAndWait { message, pos } => {
                if message.is_empty() {
                    return Err(SemanticError {
                        message: format!(
                            "Broadcast message cannot be empty at line {}, column {} in target '{}'.",
                            pos.line, pos.column, target.name
                        ),
                    });
                }
            }
            Statement::SetVar {
                var_name,
                value,
                pos,
            } => {
                ensure_variable_exists(
                    target,
                    var_name,
                    variables,
                    target_infos,
                    param_scope,
                    pos.line,
                    pos.column,
                )?;
                analyze_expr(target, value, variables, lists, target_infos, param_scope)?;
            }
            Statement::ChangeVar {
                var_name,
                delta,
                pos,
            } => {
                ensure_variable_exists(
                    target,
                    var_name,
                    variables,
                    target_infos,
                    param_scope,
                    pos.line,
                    pos.column,
                )?;
                analyze_expr(target, delta, variables, lists, target_infos, param_scope)?;
            }
            Statement::Move { steps, .. } => {
                analyze_expr(target, steps, variables, lists, target_infos, param_scope)?
            }
            Statement::Say { message, .. } => {
                analyze_expr(target, message, variables, lists, target_infos, param_scope)?
            }
            Statement::SayForSeconds {
                message, duration, ..
            } => {
                analyze_expr(target, message, variables, lists, target_infos, param_scope)?;
                analyze_expr(
                    target,
                    duration,
                    variables,
                    lists,
                    target_infos,
                    param_scope,
                )?;
            }
            Statement::Think { message, .. } => {
                analyze_expr(target, message, variables, lists, target_infos, param_scope)?
            }
            Statement::Wait { duration, .. } => analyze_expr(
                target,
                duration,
                variables,
                lists,
                target_infos,
                param_scope,
            )?,
            Statement::WaitUntil { condition, .. } => analyze_expr(
                target,
                condition,
                variables,
                lists,
                target_infos,
                param_scope,
            )?,
            Statement::Repeat { times, body, .. } => {
                analyze_expr(target, times, variables, lists, target_infos, param_scope)?;
                analyze_statements(
                    target,
                    body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::ForEach {
                var_name,
                value,
                body,
                pos,
            } => {
                ensure_variable_exists(
                    target,
                    var_name,
                    variables,
                    target_infos,
                    param_scope,
                    pos.line,
                    pos.column,
                )?;
                analyze_expr(target, value, variables, lists, target_infos, param_scope)?;
                analyze_statements(
                    target,
                    body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::While {
                condition, body, ..
            } => {
                analyze_expr(
                    target,
                    condition,
                    variables,
                    lists,
                    target_infos,
                    param_scope,
                )?;
                analyze_statements(
                    target,
                    body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::RepeatUntil {
                condition, body, ..
            } => {
                analyze_expr(
                    target,
                    condition,
                    variables,
                    lists,
                    target_infos,
                    param_scope,
                )?;
                analyze_statements(
                    target,
                    body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::Forever { body, .. } => {
                analyze_statements(
                    target,
                    body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                analyze_expr(
                    target,
                    condition,
                    variables,
                    lists,
                    target_infos,
                    param_scope,
                )?;
                analyze_statements(
                    target,
                    then_body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
                analyze_statements(
                    target,
                    else_body,
                    variables,
                    lists,
                    procedures,
                    target_infos,
                    param_scope,
                    scope_name,
                    options,
                    warnings,
                )?;
            }
            Statement::ProcedureCall { name, args, pos } => {
                if let Some(proc_info) = procedures.get(&name.to_lowercase()) {
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
                } else if let Some((remote_target_name, remote_proc_name)) = split_qualified(name) {
                    let Some(remote_target) = target_infos.get(&remote_target_name.to_lowercase())
                    else {
                        if options.allow_unknown_procedures {
                            warnings.push(SemanticWarning {
                                message: format!(
                                    "Allowed unknown procedure call '{}' at line {}, column {} in target '{}' because allow_unknown_procedures is enabled.",
                                    name, pos.line, pos.column, target.name
                                ),
                            });
                        } else {
                            return Err(SemanticError {
                                message: format!(
                                    "Unknown target '{}' in procedure call '{}' at line {}, column {} in target '{}'.",
                                    remote_target_name, name, pos.line, pos.column, target.name
                                ),
                            });
                        }
                        for arg in args {
                            analyze_expr(target, arg, variables, lists, target_infos, param_scope)?;
                        }
                        continue;
                    };
                    let Some(expected_args) = remote_target
                        .procedures
                        .get(&remote_proc_name.to_lowercase())
                    else {
                        if options.allow_unknown_procedures {
                            warnings.push(SemanticWarning {
                                message: format!(
                                    "Allowed unknown procedure call '{}' at line {}, column {} in target '{}' because allow_unknown_procedures is enabled.",
                                    name, pos.line, pos.column, target.name
                                ),
                            });
                        } else {
                            return Err(SemanticError {
                                message: format!(
                                    "Unknown procedure '{}' on target '{}' at line {}, column {} in target '{}'.",
                                    remote_proc_name, remote_target.name, pos.line, pos.column, target.name
                                ),
                            });
                        }
                        for arg in args {
                            analyze_expr(target, arg, variables, lists, target_infos, param_scope)?;
                        }
                        continue;
                    };
                    if args.len() != *expected_args {
                        return Err(SemanticError {
                            message: format!(
                                "Procedure '{}' on target '{}' expects {} argument(s), got {} at line {}, column {} in {}.",
                                remote_proc_name,
                                remote_target.name,
                                expected_args,
                                args.len(),
                                pos.line,
                                pos.column,
                                scope_name
                            ),
                        });
                    }
                } else {
                    if is_ignored_noop_call(name) {
                        for arg in args {
                            analyze_expr(target, arg, variables, lists, target_infos, param_scope)?;
                        }
                        continue;
                    }
                    if options.allow_unknown_procedures {
                        warnings.push(SemanticWarning {
                            message: format!(
                                "Allowed unknown procedure call '{}' at line {}, column {} in target '{}' because allow_unknown_procedures is enabled.",
                                name, pos.line, pos.column, target.name
                            ),
                        });
                    } else {
                        return Err(SemanticError {
                            message: format!(
                                "Unknown procedure '{}' at line {}, column {} in target '{}'.",
                                name, pos.line, pos.column, target.name
                            ),
                        });
                    }
                }
                for arg in args {
                    analyze_expr(target, arg, variables, lists, target_infos, param_scope)?;
                }
            }
            Statement::TurnRight { degrees, .. } => {
                analyze_expr(target, degrees, variables, lists, target_infos, param_scope)?
            }
            Statement::TurnLeft { degrees, .. } => {
                analyze_expr(target, degrees, variables, lists, target_infos, param_scope)?
            }
            Statement::GoToXY { x, y, .. } => {
                analyze_expr(target, x, variables, lists, target_infos, param_scope)?;
                analyze_expr(target, y, variables, lists, target_infos, param_scope)?;
            }
            Statement::GoToTarget { target: value, .. }
            | Statement::GlideToTarget { target: value, .. }
            | Statement::PointTowards { target: value, .. }
            | Statement::CreateCloneOf { target: value, .. } => {
                analyze_expr(target, value, variables, lists, target_infos, param_scope)?
            }
            Statement::GlideToXY { duration, x, y, .. } => {
                analyze_expr(
                    target,
                    duration,
                    variables,
                    lists,
                    target_infos,
                    param_scope,
                )?;
                analyze_expr(target, x, variables, lists, target_infos, param_scope)?;
                analyze_expr(target, y, variables, lists, target_infos, param_scope)?;
            }
            Statement::ChangeXBy { value, .. }
            | Statement::SetX { value, .. }
            | Statement::ChangeYBy { value, .. }
            | Statement::SetY { value, .. }
            | Statement::ChangeSizeBy { value, .. }
            | Statement::SetSizeTo { value, .. }
            | Statement::SetGraphicEffectTo { value, .. }
            | Statement::ChangeGraphicEffectBy { value, .. }
            | Statement::GoLayers { layers: value, .. }
            | Statement::ChangePenSizeBy { value, .. }
            | Statement::SetPenSizeTo { value, .. }
            | Statement::ChangePenColorParamBy { value, .. }
            | Statement::SetPenColorParamTo { value, .. }
            | Statement::SwitchCostumeTo { costume: value, .. }
            | Statement::SwitchBackdropTo {
                backdrop: value, ..
            }
            | Statement::SetSoundEffectTo { value, .. }
            | Statement::SetVolumeTo { value, .. }
            | Statement::StartSound { sound: value, .. }
            | Statement::PlaySoundUntilDone { sound: value, .. } => {
                analyze_expr(target, value, variables, lists, target_infos, param_scope)?
            }
            Statement::PointInDirection { direction, .. } => analyze_expr(
                target,
                direction,
                variables,
                lists,
                target_infos,
                param_scope,
            )?,
            Statement::IfOnEdgeBounce { .. }
            | Statement::SetRotationStyle { .. }
            | Statement::PenDown { .. }
            | Statement::PenUp { .. }
            | Statement::PenClear { .. }
            | Statement::PenStamp { .. }
            | Statement::ClearGraphicEffects { .. }
            | Statement::GoToLayer { .. }
            | Statement::Show { .. }
            | Statement::Hide { .. }
            | Statement::NextCostume { .. }
            | Statement::NextBackdrop { .. }
            | Statement::StopAllSounds { .. }
            | Statement::DeleteThisClone { .. }
            | Statement::ResetTimer { .. } => {}
            Statement::Stop { option, .. } => {
                analyze_expr(target, option, variables, lists, target_infos, param_scope)?
            }
            Statement::Ask { question, .. } => analyze_expr(
                target,
                question,
                variables,
                lists,
                target_infos,
                param_scope,
            )?,
            Statement::ShowVariable { var_name, pos }
            | Statement::HideVariable { var_name, pos } => {
                ensure_variable_exists(
                    target,
                    var_name,
                    variables,
                    target_infos,
                    param_scope,
                    pos.line,
                    pos.column,
                )?;
            }
            Statement::AddToList {
                list_name,
                item,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
                analyze_expr(target, item, variables, lists, target_infos, param_scope)?;
            }
            Statement::DeleteOfList {
                list_name,
                index,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
                analyze_expr(target, index, variables, lists, target_infos, param_scope)?;
            }
            Statement::DeleteAllOfList { list_name, pos } => {
                ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
            }
            Statement::InsertAtList {
                list_name,
                item,
                index,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
                analyze_expr(target, item, variables, lists, target_infos, param_scope)?;
                analyze_expr(target, index, variables, lists, target_infos, param_scope)?;
            }
            Statement::ReplaceItemOfList {
                list_name,
                index,
                item,
                pos,
            } => {
                ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
                analyze_expr(target, index, variables, lists, target_infos, param_scope)?;
                analyze_expr(target, item, variables, lists, target_infos, param_scope)?;
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
    target_infos: &HashMap<String, TargetInfo>,
    param_scope: &HashSet<String>,
) -> Result<(), SemanticError> {
    match expr {
        Expr::Var { name, pos } => {
            let lowered = name.to_lowercase();
            if param_scope.contains(&lowered)
                || variables.contains_key(&lowered)
                || variable_exists_anywhere(target_infos, &lowered)
            {
                return Ok(());
            }
            if let Some((remote_target_name, remote_var_name)) = split_qualified(name) {
                let Some(remote_target) = target_infos.get(&remote_target_name.to_lowercase())
                else {
                    return Err(SemanticError {
                        message: format!(
                            "Unknown target '{}' in variable reference '{}' at line {}, column {} in target '{}'.",
                            remote_target_name, name, pos.line, pos.column, target.name
                        ),
                    });
                };
                if !remote_target
                    .variables
                    .contains(&remote_var_name.to_lowercase())
                {
                    return Err(SemanticError {
                        message: format!(
                            "Unknown variable '{}' on target '{}' at line {}, column {} in target '{}'.",
                            remote_var_name, remote_target.name, pos.line, pos.column, target.name
                        ),
                    });
                }
                return Ok(());
            }
            Err(SemanticError {
                message: format!(
                    "Unknown variable '{}' at line {}, column {} in target '{}'.",
                    name, pos.line, pos.column, target.name
                ),
            })
        }
        Expr::Unary { operand, .. } => {
            analyze_expr(target, operand, variables, lists, target_infos, param_scope)
        }
        Expr::MathFunc { value, .. } => {
            analyze_expr(target, value, variables, lists, target_infos, param_scope)
        }
        Expr::Binary { left, right, .. } => {
            analyze_expr(target, left, variables, lists, target_infos, param_scope)?;
            analyze_expr(target, right, variables, lists, target_infos, param_scope)
        }
        Expr::PickRandom { start, end, .. } => {
            analyze_expr(target, start, variables, lists, target_infos, param_scope)?;
            analyze_expr(target, end, variables, lists, target_infos, param_scope)
        }
        Expr::ListItem {
            list_name,
            index,
            pos,
        } => {
            ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
            analyze_expr(target, index, variables, lists, target_infos, param_scope)
        }
        Expr::ListLength { list_name, pos } => {
            ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)
        }
        Expr::ListContents { list_name, pos } => {
            ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)
        }
        Expr::ListContains {
            list_name,
            item,
            pos,
        } => {
            ensure_list_exists(target, list_name, lists, target_infos, pos.line, pos.column)?;
            analyze_expr(target, item, variables, lists, target_infos, param_scope)
        }
        Expr::KeyPressed { key, .. } => {
            analyze_expr(target, key, variables, lists, target_infos, param_scope)
        }
        Expr::BuiltinReporter { .. } | Expr::Number { .. } | Expr::String { .. } => Ok(()),
    }
}

fn split_qualified(name: &str) -> Option<(&str, &str)> {
    let (left, right) = name.split_once('.')?;
    if left.is_empty() || right.is_empty() {
        return None;
    }
    if right.contains('.') {
        return None;
    }
    Some((left, right))
}

fn ensure_variable_exists(
    target: &Target,
    name: &str,
    variables: &HashMap<String, usize>,
    target_infos: &HashMap<String, TargetInfo>,
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
    if variables.contains_key(&lowered) || variable_exists_anywhere(target_infos, &lowered) {
        return Ok(());
    }
    Err(SemanticError {
        message: format!(
            "Unknown variable '{}' at line {}, column {} in target '{}'.",
            name, line, column, target.name
        ),
    })
}

fn ensure_list_exists(
    target: &Target,
    name: &str,
    lists: &HashMap<String, usize>,
    target_infos: &HashMap<String, TargetInfo>,
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    let lowered = name.to_lowercase();
    if lists.contains_key(&lowered) || list_exists_anywhere(target_infos, &lowered) {
        return Ok(());
    }
    Err(SemanticError {
        message: format!(
            "Unknown list '{}' at line {}, column {} in target '{}'.",
            name, line, column, target.name
        ),
    })
}

fn variable_exists_anywhere(
    target_infos: &HashMap<String, TargetInfo>,
    lowered_name: &str,
) -> bool {
    target_infos
        .values()
        .any(|target| target.variables.contains(lowered_name))
}

fn list_exists_anywhere(target_infos: &HashMap<String, TargetInfo>, lowered_name: &str) -> bool {
    target_infos
        .values()
        .any(|target| target.lists.contains(lowered_name))
}

fn is_ignored_noop_call(name: &str) -> bool {
    name.eq_ignore_ascii_case("log")
}
