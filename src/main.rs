use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, value_name = "SECONDS", default_value_t = 5)]
    time: u64,

    #[arg(short, long, value_name = "RESTART", default_value_t = false)]
    restart: bool,

    #[arg(value_name = "COMMAND", allow_hyphen_values = true, required = true)]
    command: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    if cli.verbose > 0 {
        println!("Waiting {} seconds...", cli.time);
    }
    let mut command_process = std::process::Command::new(&cli.command[0])
            .args(&cli.command[1..])
            .spawn()
            .expect("Failed to spawn command");
    if cli.verbose > 0 {
        println!("Command {:?} started OK", cli.command);
    }
    command_process
        .wait()
        .expect("Failed to wait for command");
}
