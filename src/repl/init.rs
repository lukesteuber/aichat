use super::REPL_COMMANDS;

use crate::config::{Config, SharedConfig};

use anyhow::{Context, Result};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, Emacs, FileBackedHistory, KeyCode,
    KeyModifiers, Keybindings, Prompt, PromptHistorySearch, PromptHistorySearchStatus, Reedline,
    ReedlineEvent, ReedlineMenu, ValidationResult, Validator,
};
use std::borrow::Cow;

const MENU_NAME: &str = "completion_menu";
const DEFAULT_MULTILINE_INDICATOR: &str = "::: ";

pub struct Repl {
    pub editor: Reedline,
    pub prompt: ReplPrompt,
}

impl Repl {
    pub fn init(config: SharedConfig) -> Result<Self> {
        let multiline_commands: Vec<&'static str> = REPL_COMMANDS
            .iter()
            .filter(|(_, _, v)| *v)
            .map(|(v, _, _)| *v)
            .collect();
        let completer = Self::create_completer(config.clone());
        let keybindings = Self::create_keybindings();
        let history = Self::create_history()?;
        let menu = Self::create_menu();
        let edit_mode = Box::new(Emacs::new(keybindings));
        let editor = Reedline::create()
            .with_completer(Box::new(completer))
            .with_history(history)
            .with_menu(menu)
            .with_edit_mode(edit_mode)
            .with_quick_completions(true)
            .with_partial_completions(true)
            .with_validator(Box::new(ReplValidator { multiline_commands }))
            .with_ansi_colors(true);
        let prompt = ReplPrompt(config);
        Ok(Self { editor, prompt })
    }

    fn create_completer(config: SharedConfig) -> DefaultCompleter {
        let mut completion: Vec<String> = REPL_COMMANDS
            .into_iter()
            .map(|(v, _, _)| v.to_string())
            .collect();
        completion.extend(config.lock().repl_completions());
        let mut completer = DefaultCompleter::with_inclusions(&['.', '-', '_']).set_min_word_len(2);
        completer.insert(completion.clone());
        completer
    }

    fn create_keybindings() -> Keybindings {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu(MENU_NAME.to_string()),
                ReedlineEvent::MenuNext,
            ]),
        );
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('l'),
            ReedlineEvent::ExecuteHostCommand(".clear screen".into()),
        );
        keybindings
    }

    fn create_menu() -> ReedlineMenu {
        let completion_menu = ColumnarMenu::default().with_name(MENU_NAME);
        ReedlineMenu::EngineCompleter(Box::new(completion_menu))
    }

    fn create_history() -> Result<Box<FileBackedHistory>> {
        Ok(Box::new(
            FileBackedHistory::with_file(1000, Config::history_file()?)
                .with_context(|| "Failed to setup history file")?,
        ))
    }
}

struct ReplValidator {
    multiline_commands: Vec<&'static str>,
}

impl Validator for ReplValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        if line.split('"').count() % 2 == 0 || incomplete_brackets(line, &self.multiline_commands) {
            ValidationResult::Incomplete
        } else {
            ValidationResult::Complete
        }
    }
}

fn incomplete_brackets(line: &str, multiline_commands: &[&str]) -> bool {
    let mut balance: Vec<char> = Vec::new();
    let line = line.trim_start();
    if !multiline_commands.iter().any(|v| line.starts_with(v)) {
        return false;
    }

    for c in line.chars() {
        if c == '{' {
            balance.push('}');
        } else if c == '}' {
            if let Some(last) = balance.last() {
                if last == &c {
                    balance.pop();
                }
            }
        }
    }

    !balance.is_empty()
}

#[derive(Clone)]
pub struct ReplPrompt(SharedConfig);

impl Prompt for ReplPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        let config = self.0.lock();
        if let Some(role) = config.role.as_ref() {
            role.name.to_string().into()
        } else {
            Cow::Borrowed("")
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        let config = self.0.lock();
        if let Some(conversation) = config.conversation.as_ref() {
            conversation.reamind_tokens().to_string().into()
        } else {
            Cow::Borrowed("")
        }
    }

    fn render_prompt_indicator(&self, _prompt_mode: reedline::PromptEditMode) -> Cow<str> {
        let config = self.0.lock();
        if config.conversation.is_some() {
            Cow::Borrowed("＄")
        } else {
            Cow::Borrowed("〉")
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed(DEFAULT_MULTILINE_INDICATOR)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        // NOTE: magic strings, given there is logic on how these compose I am not sure if it
        // is worth extracting in to static constant
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}
