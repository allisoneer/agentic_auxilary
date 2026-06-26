use crate::state::StageKind;
use clap::ArgGroup;
use clap::Parser;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "agentic-outer-dag")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Increase logging verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress output except errors
    #[arg(short, long)]
    pub quiet: bool,

    /// Do not run side-effecting operations.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Start {
        #[arg(long)]
        ticket: String,

        #[arg(long)]
        branch: Option<String>,

        #[arg(long)]
        worktree: Option<PathBuf>,

        #[arg(long)]
        force: bool,

        #[arg(long)]
        no_linear_handoff: bool,

        #[arg(long, value_enum)]
        stop_after: Option<StageKind>,
    },
    Resume {
        #[arg(long)]
        branch: Option<String>,

        #[arg(long)]
        worktree: Option<PathBuf>,

        #[arg(long)]
        no_linear_handoff: bool,

        #[arg(long, value_enum)]
        stop_after: Option<StageKind>,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    #[command(group(
        ArgGroup::new("decision")
            .required(true)
            .multiple(false)
            .args(["allow", "deny"])
    ))]
    RespondPermission {
        #[arg(long)]
        allow: bool,

        #[arg(long)]
        deny: bool,
    },
    RespondQuestion {
        #[arg(long)]
        answer: String,
    },
    Handoff {
        #[arg(long)]
        message: Option<String>,
    },
    Reset {
        #[arg(long)]
        yes: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use super::Commands;
    use crate::state::StageKind;
    use clap::CommandFactory;
    use clap::Parser;
    use clap::error::ErrorKind;

    #[test]
    fn generated_help_includes_expected_subcommands_and_flags() {
        let mut command = Cli::command();
        let help = command.render_long_help().to_string();

        for expected in [
            "start",
            "resume",
            "status",
            "respond-permission",
            "respond-question",
            "handoff",
            "reset",
            "--dry-run",
            "--quiet",
            "--verbose",
        ] {
            assert!(help.contains(expected), "missing help entry: {expected}");
        }
    }

    #[test]
    fn respond_permission_requires_exactly_one_flag() {
        let err = Cli::try_parse_from(["agentic-outer-dag", "respond-permission"])
            .expect_err("missing decision flag should fail");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);

        let err = Cli::try_parse_from([
            "agentic-outer-dag",
            "respond-permission",
            "--allow",
            "--deny",
        ])
        .expect_err("both decision flags should fail");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn respond_permission_accepts_allow_flag() {
        let cli = Cli::try_parse_from(["agentic-outer-dag", "respond-permission", "--allow"])
            .expect("allow should parse");
        assert!(matches!(
            cli.command,
            Commands::RespondPermission {
                allow: true,
                deny: false,
            }
        ));
    }

    #[test]
    fn respond_permission_accepts_deny_flag() {
        let cli = Cli::try_parse_from(["agentic-outer-dag", "respond-permission", "--deny"])
            .expect("deny should parse");
        assert!(matches!(
            cli.command,
            Commands::RespondPermission {
                allow: false,
                deny: true,
            }
        ));
    }

    #[test]
    fn start_accepts_top_level_dry_run_flag() {
        let cli = Cli::try_parse_from([
            "agentic-outer-dag",
            "--dry-run",
            "start",
            "--ticket",
            "ENG-992",
            "--branch",
            "feature/eng-992",
        ])
        .expect("dry-run start should parse");

        assert!(cli.dry_run);
        assert!(matches!(
            cli.command,
            Commands::Start {
                ticket,
                branch: Some(branch),
                no_linear_handoff: false,
                stop_after: None,
                ..
            } if ticket == "ENG-992" && branch == "feature/eng-992"
        ));
    }

    #[test]
    fn start_accepts_valid_stop_after_stage() {
        let cli = Cli::try_parse_from([
            "agentic-outer-dag",
            "start",
            "--ticket",
            "ENG-992",
            "--stop-after",
            "waiting_for_coderabbit",
        ])
        .expect("valid stop-after should parse");

        assert!(matches!(
            cli.command,
            Commands::Start {
                no_linear_handoff: false,
                stop_after: Some(StageKind::WaitingForCoderabbit),
                ..
            }
        ));
    }

    #[test]
    fn resume_accepts_valid_stop_after_stage() {
        let cli = Cli::try_parse_from([
            "agentic-outer-dag",
            "resume",
            "--stop-after",
            "dispatching_ticket_to_pr",
        ])
        .expect("valid stop-after should parse");

        assert!(matches!(
            cli.command,
            Commands::Resume {
                no_linear_handoff: false,
                stop_after: Some(StageKind::DispatchingTicketToPr),
                ..
            }
        ));
    }

    #[test]
    fn stop_after_rejects_invalid_stage_name() {
        let err = Cli::try_parse_from([
            "agentic-outer-dag",
            "start",
            "--ticket",
            "ENG-992",
            "--stop-after",
            "not_a_stage",
        ])
        .expect_err("invalid stop-after stage should fail");

        assert_eq!(err.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn start_help_lists_stop_after_flag_and_stage_values() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("start")
            .expect("start subcommand exists")
            .render_long_help()
            .to_string();

        assert!(help.contains("--stop-after"));
        assert!(help.contains("--no-linear-handoff"));
        assert!(help.contains("waiting_for_coderabbit"));
        assert!(help.contains("dispatching_resolve_pr_comments"));
    }

    #[test]
    fn start_accepts_no_linear_handoff_flag() {
        let cli = Cli::try_parse_from([
            "agentic-outer-dag",
            "start",
            "--ticket",
            "ENG-992",
            "--no-linear-handoff",
        ])
        .expect("start no-linear-handoff should parse");

        assert!(matches!(
            cli.command,
            Commands::Start {
                no_linear_handoff: true,
                ..
            }
        ));
    }

    #[test]
    fn resume_accepts_no_linear_handoff_flag() {
        let cli = Cli::try_parse_from(["agentic-outer-dag", "resume", "--no-linear-handoff"])
            .expect("resume no-linear-handoff should parse");

        assert!(matches!(
            cli.command,
            Commands::Resume {
                no_linear_handoff: true,
                ..
            }
        ));
    }
}
