use std::io;

use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::Shell::Zsh;

use crate::Cli;
use crate::cli::GenerateCompletionsArgs;

pub fn generate_completions(args: GenerateCompletionsArgs) {
    let shell = args.shell.unwrap_or(Zsh);
    let mut command = Cli::command();
    let bin_name = command.get_name().to_string();
    generate(shell, &mut command, bin_name, &mut io::stdout());
}
