use clap::{Parser, command};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, value_name = "SECONDS", default_value_t = 5)]
    time: u64,

    #[arg(short, long, default_value_t = false)]
    restart: bool,

    #[arg(value_name = "COMMAND", allow_hyphen_values = true, required = true)]
    command: Vec<String>,
}

enum Message {
    // Command thread to main
    CommandSender(std::sync::mpsc::Sender<Message>),
    CommandFinished, // Command died before timeout
    CommandKilled, // Confirm that the command was killed because of the timeout
    // Timeout thread to main
    Poll,
    TimeoutReached,
    // Main to thread
    StartCommand,
    PollCommand,
    KillCommand,
}

fn main() {
    let cli = Cli::parse();
    let command = cli.command;
    let verbose = cli.verbose;
    let time = cli.time;
    let restart = cli.restart;
    if verbose > 1 {
        println!("Command: {:?}", command);
        println!("Verbose: {}", verbose);
        println!("Time: {}", time);
        println!("Restart: {}", restart);
    }
    let (sender, thread_to_main_receiver) = std::sync::mpsc::channel::<Message>();
    let timeout_to_main_sender = sender.clone();
    let poll_to_main_sender = sender.clone();
    let thread_to_main_sender = sender.clone();

    let _command_thread = create_command_thread(
        command.clone(),
        verbose.clone(),
        thread_to_main_sender.clone(),
    );
    if verbose > 0 {
        println!("The main thread has started the command thread");
    }
    let _timeout_thread = std::thread::spawn(move || {
        if verbose > 1 {
            println!("Timeout thread started, timeout {} seconds", time);
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(time));
            if verbose > 1 {
                println!("Timeout message sent");
            }
            // Timetout thread sends TimeoutReached
            timeout_to_main_sender
                .send(Message::TimeoutReached)
                .expect("Failed to send timeout message");
        }
    });
    let _poll_thread = std::thread::spawn(move || {
        const POLL_TIME: u64 = 5;
        if verbose > 0 {
            println!("Poll thread started, poll time {} seconds", POLL_TIME);
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(POLL_TIME));
            // Poll thread sends Poll
            poll_to_main_sender
                .send(Message::Poll)
                .expect("Failed to send poll command message");
            if verbose > 0 {
                println!("Polling message sent");
            }
        }
    });
    // The main thread's message receiver loop
    let mut main_to_thread_sender: Option<std::sync::mpsc::Sender<Message>> = None;
    loop {
        match thread_to_main_receiver
            .recv()
            .expect("Failed to receive message from thread to main channel")
        {
            // Thread starts and sends main to thread sender endpoint
            Message::CommandSender(sender) => {
                main_to_thread_sender = Some(sender.clone());
                if verbose > 1 {
                    println!("Command sender received");
                }
                // Main sends StartCommand message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::StartCommand)
                        .expect("Failed to send start command message");
                } else {
                    panic!("Command sender received but no channel established");
                }
            }
            // Main thread gets a poll message from the poll thread
            // Poll message is sent to the command thread to check
            // if the command is still running
            Message::Poll => {
                if verbose > 1 {
                    println!("Received poll message");
                }
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::PollCommand)
                        .expect("Failed to send poll command message");
                } else {
                    panic!("Poll message received but no channel established");
                }
            }
            // Main thread gets a timeout message from the timeout thread
            // This tells that the command process should be restarted
            Message::TimeoutReached => {
                if verbose > 0 {
                    println!("Timeout reached");
                }
                if verbose > 0 {
                    println!("Killing command...");
                }
                // Main sends KillCommand message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::KillCommand)
                        .expect("Failed to send kill command message");
                } else {
                    panic!("Timeout message received but no channel established");
                }
            }
            Message::CommandFinished => {
                // TODO: status code could be useful here, to be added to protocol
                 if verbose > 1 {
                    println!("Command finished message received");
                }
                println!("Command finished before restart timeout, if you want to restart it next time, add --restart");
                if verbose > 0 {
                    println!("Exiting...");
                }
                // I'd feel safer to kill threads here but it is not possible nor necessary
                // Subthreads are killed when the main thread exits
                std::process::exit(0);
            }
            Message::CommandKilled => {
                if verbose > 1 {
                    println!("Command killed message received");
                }
                if verbose > 0 {
                    println!("Command killed because of the timeout");
                }
                if verbose > 0 {
                    println!("Restarting...");
                }
                // Main sends StartCommand message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::StartCommand)
                        .expect("Failed to send start command message");
                } else {
                    panic!("Command killed message received but no channel established");
                }
            }
            _ => panic!("Unexpected thread to main message"),
        } // End of match
    } // End of loop
}

fn create_command_thread(
    command: Vec<String>,
    verbose: u8,
    thread_to_main_sender: std::sync::mpsc::Sender<Message>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // receiver must be declared within the thread
        let (main_to_thread_sender, main_to_thread_receiver) =
            std::sync::mpsc::channel::<Message>();
        // Thread sends CommandSender
        thread_to_main_sender
            .send(Message::CommandSender(main_to_thread_sender))
            .expect("Failed to send using thread to main sender");
        let mut command_process: Option<std::process::Child> = None;
        loop {
            match main_to_thread_receiver
                .recv()
                .expect("Failed to receive message from main to thread channel")
            {
                // The command thread's message receiver loop
                Message::StartCommand => {
                    if let Some(_) = &mut command_process {
                        panic!("Tried to start a new command while another one was running");
                    }
                    command_process = Some(std::process::Command::new(&command[0])
                        .args(&command[1..])
                        .spawn()
                        .expect("Failed to spawn command"));

                    if verbose > 0 {
                        println!("The process {:?} started OK", &command);
                    }
                }
                Message::PollCommand => {
                    if let Some(p) = &mut command_process {
                        if verbose > 1 {
                            println!("Polling received in the command thread");
                        }
                        match p.try_wait() {
                            Ok(Some(status)) => {
                                // Command finished with status
                                if verbose > 0 {
                                    println!("Command {:?} finished OK", &command);
                                }
                                command_process = None;
                                // Thread sends CommandFinished
                                thread_to_main_sender
                                    .send(Message::CommandFinished)
                                    .expect("Failed to send command process finished message");
                                if verbose > 1 {
                                    println!(
                                        "Command process finished with status {}, message sent",
                                        status
                                    );
                                }
                            }
                            Ok(None) => {
                                // Command still running
                                if verbose > 1 {
                                    println!("Command still running");
                                }
                            }
                            Err(e) => {
                                panic!("Failed to poll command process: {}", e);
                            }
                        }
                    } else {
                        panic!("Tried to poll a command process while none was running");
                    }
                }
                Message::KillCommand => {
                     if let Some(mut p) = command_process {
                        p.kill().expect("Failed to kill command process");
                        p.wait().expect("Failed to reap command process");
                        command_process = None;
                    } else {
                        panic!("Tried to kill a command while none was running");
                    }
                    // Thread sends CommandKilled
                    thread_to_main_sender
                        .send(Message::CommandKilled)
                        .expect("Failed to send command process finished message");
                }
                _ => panic!("Unexpected main to thread message"),
            } // End of match
        } // End of loop
    }) // End of thread
} // End of fn
