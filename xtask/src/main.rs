use anyhow::Result;
use clap::{Parser, Subcommand};
use duct::cmd;

#[derive(Parser)]
#[command(name = "xtask")]
struct X {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start devnet
    DevnetUp,
    /// Stop devnet
    DevnetDown,
    /// Quick health check
    Health,
}

fn main() -> Result<()> {
    match X::parse().cmd {
        Cmd::DevnetUp => {
            cmd(
                "docker",
                [
                    "compose",
                    "-f",
                    "deploy/devnet/docker-compose.yml",
                    "up",
                    "-d",
                ],
            )
            .run()?;
            Ok(())
        }
        Cmd::DevnetDown => {
            cmd(
                "docker",
                [
                    "compose",
                    "-f",
                    "deploy/devnet/docker-compose.yml",
                    "down",
                    "-v",
                ],
            )
            .run()?;
            Ok(())
        }
        Cmd::Health => {
            // Probe the two RPCs quickly; replace with real endpoints once RPC exists
            cmd(
                "bash",
                [
                    "-lc",
                    "curl -sf http://localhost:9001/health && echo OK || echo FAIL",
                ],
            )
            .run()?;
            cmd(
                "bash",
                [
                    "-lc",
                    "curl -sf http://localhost:9002/health && echo OK || echo FAIL",
                ],
            )
            .run()?;
            Ok(())
        }
    }
}
