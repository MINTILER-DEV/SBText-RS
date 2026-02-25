use crate::ast::{
    CostumeDecl, EventScript, EventType, Expr, ListDecl, Position, Procedure, Project, Statement, Target, VariableDecl,
};
use crate::lexer::{Token, TokenType};
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub pos: Position,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {}, column {})", self.message, self.pos.line, self.pos.column)
    }
}

impl Error for ParseError {}

pub struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    pub fn parse_project(&mut self) -> Result<Project, ParseError> {
        self.skip_newlines();
        let start = self.current().pos;
        let mut targets = Vec::new();
        while !self.at_end() {
            let token = self.current().clone();
            if self.match_keyword("sprite") {
                targets.push(self.parse_sprite(token.pos)?);
            } else if self.match_keyword("stage") {
                targets.push(self.parse_stage(token.pos)?);
            } else {
                return self.error_here("Expected 'sprite' or 'stage'.");
            }
            self.skip_newlines();
        }
        if targets.is_empty() {
            return Err(ParseError {
                message: "Expected at least one 'stage' or 'sprite' block.".to_string(),
                pos: start,
            });
        }
        Ok(Project { pos: start, targets })
    }

    fn parse_sprite(&mut self, pos: Position) -> Result<Target, ParseError> {
        let name = self.parse_sprite_name_token()?;
        self.skip_newlines();
        self.parse_target_body(name, false, pos)
    }

    fn parse_stage(&mut self, pos: Position) -> Result<Target, ParseError> {
        let mut name = "Stage".to_string();
        if self.check_type(TokenType::Ident) || self.check_type(TokenType::String) {
            name = self.parse_name_token()?;
        }
        self.skip_newlines();
        self.parse_target_body(name, true, pos)
    }

    fn parse_target_body(&mut self, name: String, is_stage: bool, pos: Position) -> Result<Target, ParseError> {
        let mut target = Target {
            pos,
            name,
            is_stage,
            variables: Vec::new(),
            lists: Vec::new(),
            costumes: Vec::new(),
            procedures: Vec::new(),
            scripts: Vec::new(),
        };
        loop {
            self.skip_newlines();
            if self.at_end() {
                return self.error_here(format!(
                    "Unterminated target block for '{}'. Expected 'end'.",
                    target.name
                ));
            }
            if self.match_keyword("end") {
                break;
            }
            if self.match_keyword("var") {
                let prev = self.previous().pos;
                let var_name = self.parse_decl_name_token()?;
                target.variables.push(VariableDecl { pos: prev, name: var_name });
                continue;
            }
            if self.match_keyword("list") {
                let prev = self.previous().pos;
                let list_name = self.parse_decl_name_token()?;
                target.lists.push(ListDecl { pos: prev, name: list_name });
                continue;
            }
            if self.match_keyword("costume") {
                let prev = self.previous().pos;
                let path_token = self.consume_type(TokenType::String, "Expected costume path string.")?;
                target.costumes.push(CostumeDecl {
                    pos: prev,
                    path: path_token.value,
                });
                continue;
            }
            if self.match_keyword("define") {
                let prev = self.previous().pos;
                target.procedures.push(self.parse_procedure(prev)?);
                continue;
            }
            if self.match_keyword("when") {
                let prev = self.previous().pos;
                target.scripts.push(self.parse_event_script(prev)?);
                continue;
            }
            return self.error_here("Expected 'var', 'list', 'costume', 'define', 'when', or 'end' inside target.");
        }
        Ok(target)
    }

    fn parse_procedure(&mut self, pos: Position) -> Result<Procedure, ParseError> {
        let name = self.parse_name_token()?;
        let mut params = Vec::new();
        while self.check_type(TokenType::LParen) {
            self.consume_type(TokenType::LParen, "Expected '('.")?;
            if self.check_type(TokenType::RParen) {
                return self.error_here("Empty parameter declaration is not allowed.");
            }
            let param = self.parse_decl_name_token()?;
            self.consume_type(TokenType::RParen, "Expected ')' after parameter name.")?;
            params.push(param);
        }
        self.skip_newlines();
        let body = self.parse_statement_block(&["end"], false)?;
        self.consume_keyword("end", "Expected 'end' to close procedure definition.")?;
        Ok(Procedure { pos, name, params, body })
    }

    fn parse_event_script(&mut self, pos: Position) -> Result<EventScript, ParseError> {
        let event_type = if self.match_keyword("flag") {
            self.consume_keyword("clicked", "Expected 'clicked' after 'when flag'.")?;
            EventType::WhenFlagClicked
        } else if self.match_keyword("this") {
            self.consume_keyword("sprite", "Expected 'sprite' in 'when this sprite clicked'.")?;
            self.consume_keyword("clicked", "Expected 'clicked' in 'when this sprite clicked'.")?;
            EventType::WhenThisSpriteClicked
        } else if self.match_keyword("i") {
            self.consume_keyword("receive", "Expected 'receive' after 'when I'.")?;
            let msg = self.parse_bracket_text()?;
            if msg.is_empty() {
                return self.error_here("Broadcast message cannot be empty.");
            }
            EventType::WhenIReceive(msg)
        } else {
            return self.error_here("Unknown event header after 'when'.");
        };
        self.skip_newlines();
        let body = self.parse_statement_block(&["when", "define", "var", "list", "costume", "end"], false)?;
        if self.check_keyword("end") && self.looks_like_event_end() {
            self.advance();
        }
        Ok(EventScript { pos, event_type, body })
    }

    fn parse_statement_block(&mut self, until_keywords: &[&str], consume_until: bool) -> Result<Vec<Statement>, ParseError> {
        let end_set: HashSet<&str> = until_keywords.iter().copied().collect();
        let mut statements = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            let token = self.current();
            if token.typ == TokenType::Keyword && end_set.contains(token.value.as_str()) {
                if consume_until {
                    self.advance();
                }
                break;
            }
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.check_keyword("broadcast") {
            return self.parse_broadcast_stmt();
        }
        if self.check_keyword("set") {
            return self.parse_set_stmt();
        }
        if self.check_keyword("change") {
            return self.parse_change_stmt();
        }
        if self.check_keyword("move") {
            return self.parse_move_stmt();
        }
        if self.check_keyword("say") {
            return self.parse_say_stmt();
        }
        if self.check_keyword("think") {
            return self.parse_think_stmt();
        }
        if self.check_keyword("repeat") {
            return self.parse_repeat_stmt();
        }
        if self.check_keyword("forever") {
            return self.parse_forever_stmt();
        }
        if self.check_keyword("if") {
            if self.looks_like_if_on_edge_bounce() {
                return self.parse_if_on_edge_bounce_stmt();
            }
            return self.parse_if_stmt();
        }
        if self.check_keyword("turn") {
            return self.parse_turn_stmt();
        }
        if self.check_keyword("go") {
            return self.parse_go_stmt();
        }
        if self.check_keyword("point") {
            return self.parse_point_stmt();
        }
        if self.check_keyword("show") {
            return self.parse_show_stmt();
        }
        if self.check_keyword("hide") {
            return self.parse_hide_stmt();
        }
        if self.check_keyword("next") {
            return self.parse_next_stmt();
        }
        if self.check_keyword("wait") {
            return self.parse_wait_stmt();
        }
        if self.check_keyword("stop") {
            return self.parse_stop_stmt();
        }
        if self.check_keyword("ask") {
            return self.parse_ask_stmt();
        }
        if self.check_keyword("reset") {
            return self.parse_reset_stmt();
        }
        if self.check_keyword("pen") {
            return self.parse_pen_stmt();
        }
        if self.check_keyword("erase") {
            return self.parse_erase_stmt();
        }
        if self.check_keyword("stamp") {
            return self.parse_stamp_stmt();
        }
        if self.check_keyword("add") {
            return self.parse_add_to_list_stmt();
        }
        if self.check_keyword("delete") {
            return self.parse_delete_list_stmt();
        }
        if self.check_keyword("insert") {
            return self.parse_insert_list_stmt();
        }
        if self.check_keyword("replace") {
            return self.parse_replace_list_stmt();
        }
        if self.check_type(TokenType::Ident) {
            return self.parse_call_stmt();
        }
        self.error_here("Unknown statement.")
    }

    fn parse_broadcast_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("broadcast", "Expected 'broadcast'.")?.pos;
        let wait = if self.match_keyword("and") {
            self.consume_keyword("wait", "Expected 'wait' after 'broadcast and'.")?;
            true
        } else {
            false
        };
        let message = self.parse_bracket_text()?;
        if message.is_empty() {
            return self.error_here("Broadcast message cannot be empty.");
        }
        if wait {
            return Ok(Statement::BroadcastAndWait { pos: start, message });
        }
        Ok(Statement::Broadcast { pos: start, message })
    }

    fn parse_set_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("set", "Expected 'set'.")?.pos;
        if self.match_keyword("x") {
            self.consume_keyword("to", "Expected 'to' in 'set x to'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::SetX { pos: start, value });
        }
        if self.match_keyword("y") {
            self.consume_keyword("to", "Expected 'to' in 'set y to'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::SetY { pos: start, value });
        }
        if self.match_keyword("size") {
            self.consume_keyword("to", "Expected 'to' in 'set size to'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::SetSizeTo { pos: start, value });
        }
        if self.match_keyword("pen") {
            return self.parse_set_pen_stmt(start);
        }
        let var_name = self.parse_variable_field_name()?;
        self.consume_keyword("to", "Expected 'to' in set statement.")?;
        let value = self.parse_wrapped_expression()?;
        Ok(Statement::SetVar { pos: start, var_name, value })
    }

    fn parse_change_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("change", "Expected 'change'.")?.pos;
        if self.match_keyword("x") {
            self.consume_keyword("by", "Expected 'by' in 'change x by'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::ChangeXBy { pos: start, value });
        }
        if self.match_keyword("y") {
            self.consume_keyword("by", "Expected 'by' in 'change y by'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::ChangeYBy { pos: start, value });
        }
        if self.match_keyword("size") {
            self.consume_keyword("by", "Expected 'by' in 'change size by'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::ChangeSizeBy { pos: start, value });
        }
        if self.match_keyword("pen") {
            return self.parse_change_pen_stmt(start);
        }
        let var_name = self.parse_variable_field_name()?;
        self.consume_keyword("by", "Expected 'by' in change statement.")?;
        let delta = self.parse_wrapped_expression()?;
        Ok(Statement::ChangeVar { pos: start, var_name, delta })
    }

    fn parse_move_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("move", "Expected 'move'.")?.pos;
        let steps = self.parse_wrapped_expression()?;
        if !self.match_keyword("steps") && self.check_type(TokenType::LBracket) {
            let unit = self.parse_bracket_text()?;
            if !unit.eq_ignore_ascii_case("steps") {
                return self.error_here("Expected 'steps' or '[steps]' after move amount.");
            }
        }
        Ok(Statement::Move { pos: start, steps })
    }

    fn parse_say_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("say", "Expected 'say'.")?.pos;
        let message = self.parse_wrapped_expression()?;
        if self.match_keyword("for") {
            let duration = self.parse_wrapped_expression()?;
            if !self.match_keyword("seconds") && self.check_type(TokenType::LBracket) {
                let unit = self.parse_bracket_text()?;
                if !unit.eq_ignore_ascii_case("seconds") {
                    return self.error_here("Expected 'seconds' or '[seconds]' after say duration.");
                }
            }
            return Ok(Statement::SayForSeconds {
                pos: start,
                message,
                duration,
            });
        }
        Ok(Statement::Say { pos: start, message })
    }

    fn parse_think_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("think", "Expected 'think'.")?.pos;
        let message = self.parse_wrapped_expression()?;
        Ok(Statement::Think { pos: start, message })
    }

    fn parse_repeat_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("repeat", "Expected 'repeat'.")?.pos;
        if self.match_keyword("until") {
            let condition = self.parse_condition_until_newline(start, "repeat until")?;
            self.skip_newlines();
            let body = self.parse_statement_block(&["end"], false)?;
            self.consume_keyword("end", "Expected 'end' to close repeat-until block.")?;
            return Ok(Statement::RepeatUntil {
                pos: start,
                condition,
                body,
            });
        }
        let times = self.parse_wrapped_expression()?;
        self.skip_newlines();
        let body = self.parse_statement_block(&["end"], false)?;
        self.consume_keyword("end", "Expected 'end' to close repeat block.")?;
        Ok(Statement::Repeat { pos: start, times, body })
    }

    fn parse_forever_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("forever", "Expected 'forever'.")?.pos;
        self.skip_newlines();
        let body = self.parse_statement_block(&["end"], false)?;
        self.consume_keyword("end", "Expected 'end' to close forever block.")?;
        Ok(Statement::Forever { pos: start, body })
    }

    fn parse_if_on_edge_bounce_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("if", "Expected 'if'.")?.pos;
        self.consume_keyword("on", "Expected 'on' in 'if on edge bounce'.")?;
        self.consume_keyword("edge", "Expected 'edge' in 'if on edge bounce'.")?;
        self.consume_keyword("bounce", "Expected 'bounce' in 'if on edge bounce'.")?;
        Ok(Statement::IfOnEdgeBounce { pos: start })
    }

    fn parse_turn_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("turn", "Expected 'turn'.")?.pos;
        if self.match_keyword("right") {
            let degrees = self.parse_wrapped_expression()?;
            return Ok(Statement::TurnRight { pos: start, degrees });
        }
        if self.match_keyword("left") {
            let degrees = self.parse_wrapped_expression()?;
            return Ok(Statement::TurnLeft { pos: start, degrees });
        }
        self.error_here("Expected 'right' or 'left' after 'turn'.")
    }

    fn parse_go_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("go", "Expected 'go'.")?.pos;
        self.consume_keyword("to", "Expected 'to' after 'go'.")?;
        self.consume_keyword("x", "Expected 'x' in 'go to x ... y ...'.")?;
        let x = self.parse_wrapped_expression()?;
        self.consume_keyword("y", "Expected 'y' in 'go to x ... y ...'.")?;
        let y = self.parse_wrapped_expression()?;
        Ok(Statement::GoToXY { pos: start, x, y })
    }

    fn parse_point_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("point", "Expected 'point'.")?.pos;
        self.consume_keyword("in", "Expected 'in' after 'point'.")?;
        self.consume_keyword("direction", "Expected 'direction' after 'point in'.")?;
        let direction = self.parse_wrapped_expression()?;
        Ok(Statement::PointInDirection { pos: start, direction })
    }

    fn parse_show_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("show", "Expected 'show'.")?.pos;
        Ok(Statement::Show { pos: start })
    }

    fn parse_hide_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("hide", "Expected 'hide'.")?.pos;
        Ok(Statement::Hide { pos: start })
    }

    fn parse_next_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("next", "Expected 'next'.")?.pos;
        if self.match_keyword("costume") {
            return Ok(Statement::NextCostume { pos: start });
        }
        if self.match_keyword("backdrop") {
            return Ok(Statement::NextBackdrop { pos: start });
        }
        self.error_here("Expected 'costume' or 'backdrop' after 'next'.")
    }

    fn parse_wait_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("wait", "Expected 'wait'.")?.pos;
        if self.match_keyword("until") {
            let condition = self.parse_condition_until_newline(start, "wait until")?;
            return Ok(Statement::WaitUntil { pos: start, condition });
        }
        let duration = self.parse_wrapped_expression()?;
        Ok(Statement::Wait { pos: start, duration })
    }

    fn parse_condition_until_newline(&mut self, start: Position, context: &str) -> Result<Expr, ParseError> {
        let mut condition_tokens = self.collect_tokens_until_newline()?;
        if condition_tokens.is_empty() {
            return Err(ParseError {
                message: format!("Expected condition after '{}'.", context),
                pos: start,
            });
        }
        if condition_tokens[0].typ == TokenType::Op
            && condition_tokens[0].value == "<"
            && condition_tokens
                .last()
                .map(|t| t.typ == TokenType::Op && t.value == ">")
                .unwrap_or(false)
        {
            condition_tokens = condition_tokens[1..condition_tokens.len() - 1].to_vec();
        }
        self.parse_expression_from_tokens(condition_tokens)
    }

    fn parse_stop_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("stop", "Expected 'stop'.")?.pos;
        let option = self.parse_wrapped_expression()?;
        Ok(Statement::Stop { pos: start, option })
    }

    fn parse_ask_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("ask", "Expected 'ask'.")?.pos;
        let question = self.parse_wrapped_expression()?;
        Ok(Statement::Ask { pos: start, question })
    }

    fn parse_reset_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("reset", "Expected 'reset'.")?.pos;
        self.consume_keyword("timer", "Expected 'timer' after 'reset'.")?;
        Ok(Statement::ResetTimer { pos: start })
    }

    fn parse_pen_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("pen", "Expected 'pen'.")?.pos;
        if self.match_keyword("down") {
            return Ok(Statement::PenDown { pos: start });
        }
        if self.match_keyword("up") {
            return Ok(Statement::PenUp { pos: start });
        }
        self.error_here("Expected 'down' or 'up' after 'pen'.")
    }

    fn parse_erase_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("erase", "Expected 'erase'.")?.pos;
        self.consume_keyword("all", "Expected 'all' after 'erase'.")?;
        Ok(Statement::PenClear { pos: start })
    }

    fn parse_stamp_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("stamp", "Expected 'stamp'.")?.pos;
        Ok(Statement::PenStamp { pos: start })
    }

    fn parse_set_pen_stmt(&mut self, start: Position) -> Result<Statement, ParseError> {
        let param = self.parse_pen_param_name()?;
        if param == "size" {
            self.consume_keyword("to", "Expected 'to' in 'set pen size to'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::SetPenSizeTo { pos: start, value });
        }
        if is_pen_color_param(param.as_str()) {
            self.consume_keyword("to", "Expected 'to' in 'set pen <param> to'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::SetPenColorParamTo { pos: start, param, value });
        }
        self.error_here("Unknown pen parameter. Use size/color/saturation/brightness/transparency.")
    }

    fn parse_change_pen_stmt(&mut self, start: Position) -> Result<Statement, ParseError> {
        let param = self.parse_pen_param_name()?;
        if param == "size" {
            self.consume_keyword("by", "Expected 'by' in 'change pen size by'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::ChangePenSizeBy { pos: start, value });
        }
        if is_pen_color_param(param.as_str()) {
            self.consume_keyword("by", "Expected 'by' in 'change pen <param> by'.")?;
            let value = self.parse_wrapped_expression()?;
            return Ok(Statement::ChangePenColorParamBy { pos: start, param, value });
        }
        self.error_here("Unknown pen parameter. Use size/color/saturation/brightness/transparency.")
    }

    fn parse_pen_param_name(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        if token.typ == TokenType::Keyword {
            self.advance();
            return Ok(token.value);
        }
        if token.typ == TokenType::Ident {
            self.advance();
            return Ok(token.value.to_lowercase());
        }
        self.error_here("Expected pen parameter name.")
    }

    fn parse_add_to_list_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("add", "Expected 'add'.")?.pos;
        let item = self.parse_wrapped_expression()?;
        self.consume_keyword("to", "Expected 'to' in list add statement.")?;
        let list_name = self.parse_list_field_name()?;
        Ok(Statement::AddToList { pos: start, list_name, item })
    }

    fn parse_delete_list_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("delete", "Expected 'delete'.")?.pos;
        if self.match_keyword("all") {
            self.consume_keyword("of", "Expected 'of' in 'delete all of [list]'.")?;
            let list_name = self.parse_list_field_name()?;
            return Ok(Statement::DeleteAllOfList { pos: start, list_name });
        }
        let index = self.parse_wrapped_expression()?;
        self.consume_keyword("of", "Expected 'of' in list delete statement.")?;
        let list_name = self.parse_list_field_name()?;
        Ok(Statement::DeleteOfList { pos: start, list_name, index })
    }

    fn parse_insert_list_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("insert", "Expected 'insert'.")?.pos;
        let item = self.parse_wrapped_expression()?;
        self.consume_keyword("at", "Expected 'at' in list insert statement.")?;
        let index = self.parse_wrapped_expression()?;
        self.consume_keyword("of", "Expected 'of' in list insert statement.")?;
        let list_name = self.parse_list_field_name()?;
        Ok(Statement::InsertAtList {
            pos: start,
            list_name,
            item,
            index,
        })
    }

    fn parse_replace_list_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("replace", "Expected 'replace'.")?.pos;
        self.consume_keyword("item", "Expected 'item' after 'replace'.")?;
        let index = self.parse_wrapped_expression()?;
        self.consume_keyword("of", "Expected 'of' in list replace statement.")?;
        let list_name = self.parse_list_field_name()?;
        self.skip_newlines();
        self.consume_keyword("with", "Expected 'with' in list replace statement.")?;
        let item = self.parse_wrapped_expression()?;
        Ok(Statement::ReplaceItemOfList {
            pos: start,
            list_name,
            index,
            item,
        })
    }

    fn parse_if_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.consume_keyword("if", "Expected 'if'.")?.pos;
        let mut condition_tokens = self.collect_tokens_until_keyword("then")?;
        if condition_tokens.is_empty() {
            return Err(ParseError {
                message: "Expected condition after 'if'.".to_string(),
                pos: start,
            });
        }
        if condition_tokens[0].typ == TokenType::Op && condition_tokens[0].value == "<" {
            let last_is_close = condition_tokens
                .last()
                .map(|t| t.typ == TokenType::Op && t.value == ">")
                .unwrap_or(false);
            if !last_is_close {
                return Err(ParseError {
                    message: "Expected condition enclosed in '<...>' before 'then'.".to_string(),
                    pos: start,
                });
            }
            condition_tokens = condition_tokens[1..condition_tokens.len() - 1].to_vec();
        }
        let condition = self.parse_expression_from_tokens(condition_tokens)?;
        self.consume_keyword("then", "Expected 'then' in if statement.")?;
        self.skip_newlines();
        let then_body = self.parse_statement_block(&["else", "end"], false)?;
        let mut else_body = Vec::new();
        if self.match_keyword("else") {
            self.skip_newlines();
            else_body = self.parse_statement_block(&["end"], false)?;
        }
        self.consume_keyword("end", "Expected 'end' to close if statement.")?;
        Ok(Statement::If {
            pos: start,
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_call_stmt(&mut self) -> Result<Statement, ParseError> {
        let token = self.consume_type(TokenType::Ident, "Expected procedure name.")?;
        let mut args = Vec::new();
        while self.check_type(TokenType::LParen) {
            args.push(self.parse_wrapped_expression()?);
        }
        Ok(Statement::ProcedureCall {
            pos: token.pos,
            name: token.value,
            args,
        })
    }

    fn parse_wrapped_expression(&mut self) -> Result<Expr, ParseError> {
        self.consume_type(TokenType::LParen, "Expected '('.")?;
        let expr = self.parse_expression(&[TokenType::RParen], 1)?;
        self.consume_type(TokenType::RParen, "Expected ')' after expression.")?;
        Ok(expr)
    }

    fn parse_expression_from_tokens(&self, mut tokens: Vec<Token>) -> Result<Expr, ParseError> {
        let pos = tokens.last().map(|t| t.pos).unwrap_or(Position::new(1, 1));
        tokens.push(Token {
            typ: TokenType::Eof,
            value: String::new(),
            pos,
        });
        let mut parser = Parser::new(tokens);
        let expr = parser.parse_expression(&[TokenType::Eof], 1)?;
        parser.consume_type(TokenType::Eof, "Unexpected trailing tokens in expression.")?;
        Ok(expr)
    }

    fn parse_expression(&mut self, stop_types: &[TokenType], min_precedence: i32) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary(stop_types)?;
        loop {
            let token = self.current().clone();
            if stop_types.contains(&token.typ) {
                break;
            }
            let Some(op) = self.as_operator(&token) else {
                break;
            };
            let Some(precedence) = precedence_of(&op) else {
                break;
            };
            if precedence < min_precedence {
                break;
            }
            self.advance();
            let right = self.parse_expression(stop_types, precedence + 1)?;
            left = Expr::Binary {
                pos: token.pos,
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self, stop_types: &[TokenType]) -> Result<Expr, ParseError> {
        let token = self.current().clone();
        if token.typ == TokenType::Op && token.value == "-" {
            self.advance();
            let operand = self.parse_unary(stop_types)?;
            return Ok(Expr::Unary {
                pos: token.pos,
                op: "-".to_string(),
                operand: Box::new(operand),
            });
        }
        if token.typ == TokenType::Keyword && token.value == "not" {
            self.advance();
            let operand = self.parse_unary(stop_types)?;
            return Ok(Expr::Unary {
                pos: token.pos,
                op: "not".to_string(),
                operand: Box::new(operand),
            });
        }
        self.parse_primary(stop_types)
    }

    fn parse_primary(&mut self, stop_types: &[TokenType]) -> Result<Expr, ParseError> {
        let token = self.current().clone();
        if stop_types.contains(&token.typ) {
            return self.error_here("Expected expression.");
        }
        if self.check_keyword("pick") {
            return self.parse_pick_random_expr();
        }
        if self.check_keyword("item") {
            return self.parse_item_of_list_expr();
        }
        if self.check_keyword("length") {
            return self.parse_length_expr();
        }
        if self.check_keyword("key") {
            return self.parse_key_pressed_expr();
        }
        if self.check_keyword("floor") {
            return self.parse_math_func_expr("floor");
        }
        if self.check_keyword("round") {
            return self.parse_math_func_expr("round");
        }
        if self.check_keyword("answer") {
            let start = self.consume_keyword("answer", "Expected 'answer'.")?.pos;
            return Ok(Expr::BuiltinReporter {
                pos: start,
                kind: "answer".to_string(),
            });
        }
        if self.check_keyword("mouse") {
            let start = self.consume_keyword("mouse", "Expected 'mouse'.")?.pos;
            if self.match_keyword("x") {
                return Ok(Expr::BuiltinReporter {
                    pos: start,
                    kind: "mouse_x".to_string(),
                });
            }
            if self.match_keyword("y") {
                return Ok(Expr::BuiltinReporter {
                    pos: start,
                    kind: "mouse_y".to_string(),
                });
            }
            return self.error_here("Expected 'x' or 'y' after 'mouse'.");
        }
        if self.check_keyword("timer") {
            let start = self.consume_keyword("timer", "Expected 'timer'.")?.pos;
            return Ok(Expr::BuiltinReporter {
                pos: start,
                kind: "timer".to_string(),
            });
        }
        if token.typ == TokenType::Number {
            self.advance();
            let value = token.value.parse::<f64>().unwrap_or(0.0);
            return Ok(Expr::Number {
                pos: token.pos,
                value,
            });
        }
        if token.typ == TokenType::String {
            self.advance();
            return Ok(Expr::String {
                pos: token.pos,
                value: token.value,
            });
        }
        if token.typ == TokenType::Ident {
            if self.peek().typ == TokenType::LParen {
                return Err(ParseError {
                    message: format!("Procedure call '{}' cannot appear inside an expression.", token.value),
                    pos: token.pos,
                });
            }
            self.advance();
            return Ok(Expr::Var {
                pos: token.pos,
                name: token.value,
            });
        }
        if token.typ == TokenType::Keyword {
            self.advance();
            return Ok(Expr::Var {
                pos: token.pos,
                name: token.value,
            });
        }
        if token.typ == TokenType::LParen {
            self.advance();
            let expr = self.parse_expression(&[TokenType::RParen], 1)?;
            self.consume_type(TokenType::RParen, "Expected ')' after grouped expression.")?;
            return Ok(expr);
        }
        if token.typ == TokenType::LBracket {
            let name = self.parse_variable_field_name()?;
            if self.match_keyword("contains") {
                let item = self.parse_wrapped_expression()?;
                return Ok(Expr::ListContains {
                    pos: token.pos,
                    list_name: name,
                    item: Box::new(item),
                });
            }
            return Ok(Expr::Var {
                pos: token.pos,
                name,
            });
        }
        self.error_here("Expected expression.")
    }

    fn parse_pick_random_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.consume_keyword("pick", "Expected 'pick'.")?.pos;
        self.consume_keyword("random", "Expected 'random' after 'pick'.")?;
        let low = self.parse_wrapped_expression()?;
        self.consume_keyword("to", "Expected 'to' in 'pick random ... to ...'.")?;
        let high = self.parse_wrapped_expression()?;
        Ok(Expr::PickRandom {
            pos: start,
            start: Box::new(low),
            end: Box::new(high),
        })
    }

    fn parse_item_of_list_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.consume_keyword("item", "Expected 'item'.")?.pos;
        let index = self.parse_wrapped_expression()?;
        self.consume_keyword("of", "Expected 'of' in 'item (...) of [list]'.")?;
        let list_name = self.parse_list_field_name()?;
        Ok(Expr::ListItem {
            pos: start,
            list_name,
            index: Box::new(index),
        })
    }

    fn parse_length_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.consume_keyword("length", "Expected 'length'.")?.pos;
        self.consume_keyword("of", "Expected 'of' in 'length of ...'.")?;
        if self.check_type(TokenType::LBracket) {
            let list_name = self.parse_list_field_name()?;
            return Ok(Expr::ListLength { pos: start, list_name });
        }
        self.error_here("Expected list reference after 'length of'.")
    }

    fn parse_key_pressed_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.consume_keyword("key", "Expected 'key'.")?.pos;
        let key = self.parse_wrapped_expression()?;
        let word = self.current_word();
        if word.as_deref() == Some("pressed") || word.as_deref() == Some("pressed?") {
            self.advance();
        } else {
            return self.error_here("Expected 'pressed?' in key sensing expression.");
        }
        Ok(Expr::KeyPressed {
            pos: start,
            key: Box::new(key),
        })
    }

    fn parse_math_func_expr(&mut self, op: &str) -> Result<Expr, ParseError> {
        let start = self.consume_keyword(op, format!("Expected '{}'.", op).as_str())?.pos;
        let value = self.parse_wrapped_expression()?;
        Ok(Expr::MathFunc {
            pos: start,
            op: op.to_string(),
            value: Box::new(value),
        })
    }

    fn parse_variable_field_name(&mut self) -> Result<String, ParseError> {
        let mut contents = self.parse_bracket_tokens()?;
        if contents.is_empty() {
            return self.error_here("Variable name cannot be empty.");
        }
        if contents
            .first()
            .map(|t| t.value.eq_ignore_ascii_case("var"))
            .unwrap_or(false)
        {
            contents.remove(0);
        }
        let name = contents
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if name.is_empty() {
            return self.error_here("Variable name cannot be empty.");
        }
        Ok(name)
    }

    fn parse_list_field_name(&mut self) -> Result<String, ParseError> {
        let contents = self.parse_bracket_tokens()?;
        if contents.is_empty() {
            return self.error_here("List name cannot be empty.");
        }
        let name = contents
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if name.is_empty() {
            return self.error_here("List name cannot be empty.");
        }
        Ok(name)
    }

    fn parse_bracket_text(&mut self) -> Result<String, ParseError> {
        let contents = self.parse_bracket_tokens()?;
        Ok(contents
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string())
    }

    fn parse_bracket_tokens(&mut self) -> Result<Vec<Token>, ParseError> {
        self.consume_type(TokenType::LBracket, "Expected '['.")?;
        let mut tokens = Vec::new();
        while !self.at_end() && !self.check_type(TokenType::RBracket) {
            if self.check_type(TokenType::Newline) {
                return self.error_here("Unexpected newline in bracket expression.");
            }
            tokens.push(self.advance());
        }
        self.consume_type(TokenType::RBracket, "Expected ']'.")?;
        Ok(tokens)
    }

    fn collect_tokens_until_keyword(&mut self, keyword: &str) -> Result<Vec<Token>, ParseError> {
        let mut out = Vec::new();
        let mut depth_paren: i32 = 0;
        let mut depth_bracket: i32 = 0;
        while !self.at_end() {
            let token = self.current().clone();
            if token.typ == TokenType::Keyword
                && token.value == keyword
                && depth_paren == 0
                && depth_bracket == 0
            {
                break;
            }
            match token.typ {
                TokenType::LParen => depth_paren += 1,
                TokenType::RParen => depth_paren -= 1,
                TokenType::LBracket => depth_bracket += 1,
                TokenType::RBracket => depth_bracket -= 1,
                _ => {}
            }
            out.push(self.advance());
        }
        if depth_paren != 0 || depth_bracket != 0 {
            return self.error_here("Unbalanced delimiters while reading condition.");
        }
        Ok(out)
    }

    fn collect_tokens_until_newline(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut out = Vec::new();
        let mut depth_paren: i32 = 0;
        let mut depth_bracket: i32 = 0;
        while !self.at_end() {
            let token = self.current().clone();
            if token.typ == TokenType::Newline && depth_paren == 0 && depth_bracket == 0 {
                break;
            }
            match token.typ {
                TokenType::LParen => depth_paren += 1,
                TokenType::RParen => depth_paren -= 1,
                TokenType::LBracket => depth_bracket += 1,
                TokenType::RBracket => depth_bracket -= 1,
                _ => {}
            }
            out.push(self.advance());
        }
        if depth_paren != 0 || depth_bracket != 0 {
            return self.error_here("Unbalanced delimiters while reading condition.");
        }
        Ok(out)
    }

    fn parse_name_token(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        if token.typ == TokenType::Ident || token.typ == TokenType::String {
            self.advance();
            return Ok(token.value);
        }
        self.error_here("Expected name.")
    }

    fn parse_decl_name_token(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        if token.typ == TokenType::Ident || token.typ == TokenType::String || token.typ == TokenType::Keyword {
            self.advance();
            return Ok(token.value);
        }
        self.error_here("Expected name.")
    }

    fn parse_sprite_name_token(&mut self) -> Result<String, ParseError> {
        if self.check_keyword("stage") {
            self.advance();
            return Ok("Stage".to_string());
        }
        self.parse_name_token()
    }

    fn as_operator(&self, token: &Token) -> Option<String> {
        if token.typ == TokenType::Op {
            return Some(token.value.clone());
        }
        if token.typ == TokenType::Keyword && (token.value == "and" || token.value == "or") {
            return Some(token.value.clone());
        }
        None
    }

    fn looks_like_if_on_edge_bounce(&self) -> bool {
        self.word_at_offset(0).as_deref() == Some("if")
            && self.word_at_offset(1).as_deref() == Some("on")
            && self.word_at_offset(2).as_deref() == Some("edge")
            && self.word_at_offset(3).as_deref() == Some("bounce")
    }

    fn current_word(&self) -> Option<String> {
        self.word_from_token(self.current())
    }

    fn word_at_offset(&self, offset: usize) -> Option<String> {
        self.tokens
            .get(self.index + offset)
            .and_then(|t| self.word_from_token(t))
    }

    fn word_from_token(&self, token: &Token) -> Option<String> {
        match token.typ {
            TokenType::Keyword => Some(token.value.clone()),
            TokenType::Ident => Some(token.value.to_lowercase()),
            _ => None,
        }
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        let token = self.current();
        token.typ == TokenType::Keyword && token.value == keyword
    }

    fn looks_like_event_end(&self) -> bool {
        let mut idx = self.index + 1;
        while idx < self.tokens.len() && self.tokens[idx].typ == TokenType::Newline {
            idx += 1;
        }
        if idx >= self.tokens.len() {
            return false;
        }
        let token = &self.tokens[idx];
        if token.typ == TokenType::Eof {
            return false;
        }
        if token.typ == TokenType::Keyword && (token.value == "sprite" || token.value == "stage") {
            return false;
        }
        true
    }

    fn consume_keyword(&mut self, keyword: &str, message: &str) -> Result<Token, ParseError> {
        let token = self.current().clone();
        if token.typ == TokenType::Keyword && token.value == keyword {
            self.advance();
            Ok(token)
        } else {
            Err(ParseError {
                message: message.to_string(),
                pos: token.pos,
            })
        }
    }

    fn consume_type(&mut self, typ: TokenType, message: &str) -> Result<Token, ParseError> {
        let token = self.current().clone();
        if token.typ == typ {
            self.advance();
            Ok(token)
        } else {
            Err(ParseError {
                message: message.to_string(),
                pos: token.pos,
            })
        }
    }

    fn match_keyword(&mut self, keyword: &str) -> bool {
        if self.check_keyword(keyword) {
            self.advance();
            return true;
        }
        false
    }

    fn check_type(&self, typ: TokenType) -> bool {
        self.current().typ == typ
    }

    fn skip_newlines(&mut self) {
        while self.check_type(TokenType::Newline) {
            self.advance();
        }
    }

    fn at_end(&self) -> bool {
        self.current().typ == TokenType::Eof
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn peek(&self) -> &Token {
        if self.index + 1 >= self.tokens.len() {
            &self.tokens[self.tokens.len() - 1]
        } else {
            &self.tokens[self.index + 1]
        }
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        self.index += 1;
        token
    }

    fn error_here<T: Into<String>, R>(&self, message: T) -> Result<R, ParseError> {
        Err(ParseError {
            message: message.into(),
            pos: self.current().pos,
        })
    }
}

fn precedence_of(op: &str) -> Option<i32> {
    match op {
        "or" => Some(1),
        "and" => Some(2),
        "=" | "==" | "!=" | "<" | "<=" | ">" | ">=" => Some(3),
        "+" | "-" => Some(4),
        "*" | "/" | "%" => Some(5),
        _ => None,
    }
}

fn is_pen_color_param(name: &str) -> bool {
    matches!(name, "color" | "saturation" | "brightness" | "transparency")
}
