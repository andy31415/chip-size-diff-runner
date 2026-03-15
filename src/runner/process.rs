use eyre::{Result, WrapErr, eyre};
use std::process::{Child, Command, Stdio};

/// A chain of commands piped together: stdout of each feeds stdin of the next.
#[derive(Debug)]
pub struct CommandChain {
    commands: Vec<Command>,
}

impl CommandChain {
    /// Creates a new chain from a fully configured initial command.
    pub fn new(initial_command: Command) -> Self {
        CommandChain {
            commands: vec![initial_command],
        }
    }

    /// Appends a command to the end of the pipe chain.
    pub fn pipe(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }

    /// Executes the chain, piping stdout of each command into stdin of the next.
    ///
    /// Consumes `self` — a chain is single-use since stdout handles are taken
    /// during execution.
    pub fn execute(mut self) -> Result<()> {
        let n = self.commands.len();
        if n == 0 {
            return Ok(());
        }

        let mut previous_child: Option<Child> = None;
        let mut intermediate_children: Vec<Child> = Vec::new();

        for (i, command) in self.commands.iter_mut().enumerate() {
            if let Some(mut child) = previous_child.take() {
                command.stdin(Stdio::from(child.stdout.take().unwrap()));
                intermediate_children.push(child);
            }

            if i == n - 1 {
                command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
                let status = command.status().wrap_err("Failed to execute command")?;
                if !status.success() {
                    return Err(eyre!("Command failed with status: {}", status));
                }
            } else {
                command.stdout(Stdio::piped()).stderr(Stdio::inherit());
                previous_child = Some(command.spawn().wrap_err("Failed to start command")?);
            }
        }

        // Reap intermediate children and surface any failures.
        for mut child in intermediate_children {
            let status = child
                .wait()
                .wrap_err("Failed to wait on intermediate command")?;
            if !status.success() {
                return Err(eyre!("Intermediate command failed with status: {}", status));
            }
        }

        Ok(())
    }
}
