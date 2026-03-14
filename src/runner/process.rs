use eyre::{Result, eyre, WrapErr};
use std::process::{Child, Command, Stdio};

/// Represents a chain of commands to be piped together.
pub struct CommandChain {
    pub commands: Vec<Command>,
}

impl CommandChain {
    /// Creates a new CommandChain with the initial command.
    pub fn new(initial_command: Command) -> Self {
        CommandChain {
            commands: vec![initial_command],
        }
    }

    /// Adds a command to the end of the pipe chain.
    pub fn pipe(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }

    /// Executes the command chain, piping stdout of each command to stdin of the next.
    /// The last command's stdout/stderr are inherited.
    pub fn execute(&mut self) -> Result<()> {
        if self.commands.is_empty() {
            return Ok(());
        }

        let mut previous_child: Option<Child> = None;

        for i in 0..self.commands.len() {
            let is_last = i == self.commands.len() - 1;
            let command = &mut self.commands[i];

            if let Some(mut child) = previous_child.take() {
                command.stdin(Stdio::from(child.stdout.take().unwrap()));
            }

            if is_last {
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

        Ok(())
    }
}
