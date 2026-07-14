use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Example workspace binary that depends on the shared core crate."
)]
struct Args {
    /// The project name to print in the greeting.
    #[arg(long, default_value = "workspace-cli")]
    project_name: String,
}

fn main() {
    let args = Args::parse();
    println!("{}", workspace_core::render_greeting(&args.project_name));
}
