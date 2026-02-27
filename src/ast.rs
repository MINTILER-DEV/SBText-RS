#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number {
        pos: Position,
        value: f64,
    },
    String {
        pos: Position,
        value: String,
    },
    Var {
        pos: Position,
        name: String,
    },
    PickRandom {
        pos: Position,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    ListItem {
        pos: Position,
        list_name: String,
        index: Box<Expr>,
    },
    ListLength {
        pos: Position,
        list_name: String,
    },
    ListContains {
        pos: Position,
        list_name: String,
        item: Box<Expr>,
    },
    ListContents {
        pos: Position,
        list_name: String,
    },
    KeyPressed {
        pos: Position,
        key: Box<Expr>,
    },
    TouchingObject {
        pos: Position,
        target: Box<Expr>,
    },
    TouchingColor {
        pos: Position,
        color: Box<Expr>,
    },
    BuiltinReporter {
        pos: Position,
        kind: String,
    },
    MathFunc {
        pos: Position,
        op: String,
        value: Box<Expr>,
    },
    Unary {
        pos: Position,
        op: String,
        operand: Box<Expr>,
    },
    Binary {
        pos: Position,
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

impl Expr {
    pub fn pos(&self) -> Position {
        match self {
            Expr::Number { pos, .. }
            | Expr::String { pos, .. }
            | Expr::Var { pos, .. }
            | Expr::PickRandom { pos, .. }
            | Expr::ListItem { pos, .. }
            | Expr::ListLength { pos, .. }
            | Expr::ListContains { pos, .. }
            | Expr::ListContents { pos, .. }
            | Expr::KeyPressed { pos, .. }
            | Expr::TouchingObject { pos, .. }
            | Expr::TouchingColor { pos, .. }
            | Expr::BuiltinReporter { pos, .. }
            | Expr::MathFunc { pos, .. }
            | Expr::Unary { pos, .. }
            | Expr::Binary { pos, .. } => *pos,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    Broadcast {
        pos: Position,
        message: String,
    },
    BroadcastAndWait {
        pos: Position,
        message: String,
    },
    SetVar {
        pos: Position,
        var_name: String,
        value: Expr,
    },
    ChangeVar {
        pos: Position,
        var_name: String,
        delta: Expr,
    },
    Move {
        pos: Position,
        steps: Expr,
    },
    Say {
        pos: Position,
        message: Expr,
    },
    SayForSeconds {
        pos: Position,
        message: Expr,
        duration: Expr,
    },
    Think {
        pos: Position,
        message: Expr,
    },
    Wait {
        pos: Position,
        duration: Expr,
    },
    WaitUntil {
        pos: Position,
        condition: Expr,
    },
    Repeat {
        pos: Position,
        times: Expr,
        body: Vec<Statement>,
    },
    ForEach {
        pos: Position,
        var_name: String,
        value: Expr,
        body: Vec<Statement>,
    },
    While {
        pos: Position,
        condition: Expr,
        body: Vec<Statement>,
    },
    RepeatUntil {
        pos: Position,
        condition: Expr,
        body: Vec<Statement>,
    },
    Forever {
        pos: Position,
        body: Vec<Statement>,
    },
    If {
        pos: Position,
        condition: Expr,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    ProcedureCall {
        pos: Position,
        name: String,
        args: Vec<Expr>,
    },
    TurnRight {
        pos: Position,
        degrees: Expr,
    },
    TurnLeft {
        pos: Position,
        degrees: Expr,
    },
    GoToXY {
        pos: Position,
        x: Expr,
        y: Expr,
    },
    GoToTarget {
        pos: Position,
        target: Expr,
    },
    GlideToXY {
        pos: Position,
        duration: Expr,
        x: Expr,
        y: Expr,
    },
    GlideToTarget {
        pos: Position,
        duration: Expr,
        target: Expr,
    },
    ChangeXBy {
        pos: Position,
        value: Expr,
    },
    SetX {
        pos: Position,
        value: Expr,
    },
    ChangeYBy {
        pos: Position,
        value: Expr,
    },
    SetY {
        pos: Position,
        value: Expr,
    },
    PointInDirection {
        pos: Position,
        direction: Expr,
    },
    PointTowards {
        pos: Position,
        target: Expr,
    },
    SetRotationStyle {
        pos: Position,
        style: String,
    },
    IfOnEdgeBounce {
        pos: Position,
    },
    ChangeSizeBy {
        pos: Position,
        value: Expr,
    },
    SetSizeTo {
        pos: Position,
        value: Expr,
    },
    ClearGraphicEffects {
        pos: Position,
    },
    SetGraphicEffectTo {
        pos: Position,
        effect: String,
        value: Expr,
    },
    ChangeGraphicEffectBy {
        pos: Position,
        effect: String,
        value: Expr,
    },
    GoToLayer {
        pos: Position,
        layer: String,
    },
    GoLayers {
        pos: Position,
        direction: String,
        layers: Expr,
    },
    PenDown {
        pos: Position,
    },
    PenUp {
        pos: Position,
    },
    PenClear {
        pos: Position,
    },
    PenStamp {
        pos: Position,
    },
    ChangePenSizeBy {
        pos: Position,
        value: Expr,
    },
    SetPenSizeTo {
        pos: Position,
        value: Expr,
    },
    ChangePenColorParamBy {
        pos: Position,
        param: String,
        value: Expr,
    },
    SetPenColorParamTo {
        pos: Position,
        param: String,
        value: Expr,
    },
    Show {
        pos: Position,
    },
    Hide {
        pos: Position,
    },
    NextCostume {
        pos: Position,
    },
    NextBackdrop {
        pos: Position,
    },
    SwitchCostumeTo {
        pos: Position,
        costume: Expr,
    },
    SwitchBackdropTo {
        pos: Position,
        backdrop: Expr,
    },
    Stop {
        pos: Position,
        option: Expr,
    },
    Ask {
        pos: Position,
        question: Expr,
    },
    StartSound {
        pos: Position,
        sound: Expr,
    },
    PlaySoundUntilDone {
        pos: Position,
        sound: Expr,
    },
    StopAllSounds {
        pos: Position,
    },
    SetSoundEffectTo {
        pos: Position,
        effect: String,
        value: Expr,
    },
    SetVolumeTo {
        pos: Position,
        value: Expr,
    },
    CreateCloneOf {
        pos: Position,
        target: Expr,
    },
    DeleteThisClone {
        pos: Position,
    },
    ShowVariable {
        pos: Position,
        var_name: String,
    },
    HideVariable {
        pos: Position,
        var_name: String,
    },
    ResetTimer {
        pos: Position,
    },
    AddToList {
        pos: Position,
        list_name: String,
        item: Expr,
    },
    DeleteOfList {
        pos: Position,
        list_name: String,
        index: Expr,
    },
    DeleteAllOfList {
        pos: Position,
        list_name: String,
    },
    InsertAtList {
        pos: Position,
        list_name: String,
        item: Expr,
        index: Expr,
    },
    ReplaceItemOfList {
        pos: Position,
        list_name: String,
        index: Expr,
        item: Expr,
    },
}

impl Statement {
    pub fn pos(&self) -> Position {
        match self {
            Statement::Broadcast { pos, .. }
            | Statement::BroadcastAndWait { pos, .. }
            | Statement::SetVar { pos, .. }
            | Statement::ChangeVar { pos, .. }
            | Statement::Move { pos, .. }
            | Statement::Say { pos, .. }
            | Statement::SayForSeconds { pos, .. }
            | Statement::Think { pos, .. }
            | Statement::Wait { pos, .. }
            | Statement::WaitUntil { pos, .. }
            | Statement::Repeat { pos, .. }
            | Statement::ForEach { pos, .. }
            | Statement::While { pos, .. }
            | Statement::RepeatUntil { pos, .. }
            | Statement::Forever { pos, .. }
            | Statement::If { pos, .. }
            | Statement::ProcedureCall { pos, .. }
            | Statement::TurnRight { pos, .. }
            | Statement::TurnLeft { pos, .. }
            | Statement::GoToXY { pos, .. }
            | Statement::GoToTarget { pos, .. }
            | Statement::GlideToXY { pos, .. }
            | Statement::GlideToTarget { pos, .. }
            | Statement::ChangeXBy { pos, .. }
            | Statement::SetX { pos, .. }
            | Statement::ChangeYBy { pos, .. }
            | Statement::SetY { pos, .. }
            | Statement::PointInDirection { pos, .. }
            | Statement::PointTowards { pos, .. }
            | Statement::SetRotationStyle { pos, .. }
            | Statement::IfOnEdgeBounce { pos, .. }
            | Statement::ChangeSizeBy { pos, .. }
            | Statement::SetSizeTo { pos, .. }
            | Statement::ClearGraphicEffects { pos, .. }
            | Statement::SetGraphicEffectTo { pos, .. }
            | Statement::ChangeGraphicEffectBy { pos, .. }
            | Statement::GoToLayer { pos, .. }
            | Statement::GoLayers { pos, .. }
            | Statement::PenDown { pos, .. }
            | Statement::PenUp { pos, .. }
            | Statement::PenClear { pos, .. }
            | Statement::PenStamp { pos, .. }
            | Statement::ChangePenSizeBy { pos, .. }
            | Statement::SetPenSizeTo { pos, .. }
            | Statement::ChangePenColorParamBy { pos, .. }
            | Statement::SetPenColorParamTo { pos, .. }
            | Statement::Show { pos, .. }
            | Statement::Hide { pos, .. }
            | Statement::NextCostume { pos, .. }
            | Statement::NextBackdrop { pos, .. }
            | Statement::SwitchCostumeTo { pos, .. }
            | Statement::SwitchBackdropTo { pos, .. }
            | Statement::Stop { pos, .. }
            | Statement::Ask { pos, .. }
            | Statement::StartSound { pos, .. }
            | Statement::PlaySoundUntilDone { pos, .. }
            | Statement::StopAllSounds { pos, .. }
            | Statement::SetSoundEffectTo { pos, .. }
            | Statement::SetVolumeTo { pos, .. }
            | Statement::CreateCloneOf { pos, .. }
            | Statement::DeleteThisClone { pos, .. }
            | Statement::ShowVariable { pos, .. }
            | Statement::HideVariable { pos, .. }
            | Statement::ResetTimer { pos, .. }
            | Statement::AddToList { pos, .. }
            | Statement::DeleteOfList { pos, .. }
            | Statement::DeleteAllOfList { pos, .. }
            | Statement::InsertAtList { pos, .. }
            | Statement::ReplaceItemOfList { pos, .. } => *pos,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EventType {
    WhenFlagClicked,
    WhenThisSpriteClicked,
    WhenIReceive(String),
    WhenKeyPressed(String),
}

#[derive(Debug, Clone)]
pub struct EventScript {
    pub pos: Position,
    pub event_type: EventType,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Procedure {
    pub pos: Position,
    pub name: String,
    pub params: Vec<String>,
    pub run_without_screen_refresh: bool,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct CostumeDecl {
    pub pos: Position,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum InitialValue {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub pos: Position,
    pub name: String,
    pub initial_value: Option<InitialValue>,
}

#[derive(Debug, Clone)]
pub struct ListDecl {
    pub pos: Position,
    pub name: String,
    pub initial_items: Option<Vec<InitialValue>>,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub pos: Position,
    pub name: String,
    pub is_stage: bool,
    pub variables: Vec<VariableDecl>,
    pub lists: Vec<ListDecl>,
    pub costumes: Vec<CostumeDecl>,
    pub procedures: Vec<Procedure>,
    pub scripts: Vec<EventScript>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub pos: Position,
    pub targets: Vec<Target>,
}
