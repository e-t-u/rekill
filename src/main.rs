use clap::Parser;

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
        if verbose > 0 {
            println!("Timeout thread started, timeout {} seconds", time);
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            // if verbose > 1 {
            //     println!("Timeout message sent");
            // }
            timeout_to_main_sender
                .send(Message::TimeoutReached)
                .expect("Failed to send timeout message");
        }
    });
    let _poll_thread = std::thread::spawn(move || {
        if verbose > 0 {
            println!("Poll thread started, poll time {} seconds", 1);
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            poll_to_main_sender
                .send(Message::PollCommand)
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
            Message::CommandSender(sender) => {
                // assert!(
                //     main_to_thread_sender.is_none(),
                //     "Duplicate command sender message"
                // );
                main_to_thread_sender = Some(sender);
                if verbose > 1 {
                    println!("Command sender received");
                }
                main_to_thread_sender
                    .unwrap()
                    .send(Message::StartCommand)
                    .expect("Failed to send start command message");
            }
            Message::Poll => {
                if verbose > 1 {
                    println!("Received poll message");
                }
                // assert!(main_to_thread_sender.is_some(), "Thread sender is missing");
                // main_to_thread_sender
                //     .unwrap()
                //     .send(Message::PollCommand)
                //     .expect("Failed to send kill command message");
            }
            Message::TimeoutReached => {
                if verbose > 0 {
                    println!("Timeout reached");
                }
                if verbose > 0 {
                    println!("Killing command...");
                }
                // assert!(main_to_thread_sender.is_some(), "Thread sender is missing");
                // main_to_thread_sender
                //     .unwrap()
                //     .send(Message::KillCommand)
                //     .expect("Failed to send kill command message");
            }
            Message::CommandFinished => {
                // TODO: status code could be useful here, to be added to protocol
                // this case should be a separate message:
                if verbose > 1 {
                    println!("Command finished message received");
                }
                println!("Command finished before timeout, if you want to restart it next time, add --restart");
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
                // assert!(main_to_thread_sender.is_some(), "Thread sender is missing");
                // main_to_thread_sender
                //     .unwrap()
                //     .send(Message::KillCommand)
                //     .expect("Failed to send start command message (restart)");
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
        thread_to_main_sender
            .send(Message::CommandSender(main_to_thread_sender))
            .expect("Failed to send using thread to main sender");
        let mut command_process: Option<std::process::Child> = std::process::Child::new(None);
        loop {
            match main_to_thread_receiver
                .recv()
                .expect("Failed to receive message from main to thread channel")
            {
                // The command thread's message receiver loop
                Message::StartCommand => {
                    // if command_process.is_some() {
                    //     panic!("Tried to start a new command while another one was running");
                    // }
                    command_process = Some(create_command_process(&command));
                    if verbose > 0 {
                        println!("The process {:?} started OK", &command);
                    }
                }
                Message::PollCommand => {
                    if command_process.is_none() {
                        panic!("Tried to poll a command process while none was running");
                    }
                    match command_process.unwrap().try_wait() {
                        Ok(Some(status)) => {
                            // Command finished with status
                            if verbose > 0 {
                                println!("Command {:?} finished OK", &command);
                            }
                            command_process = None;
                            thread_to_main_sender
                                .send(Message::CommandFinished)
                                .expect("Failed to send command process finished message");
                            if verbose > 0 {
                                println!(
                                    "Command process finished with status {}, message sent",
                                    status
                                );
                            }
                            std::process::exit(0);
                        }
                        Ok(None) => {
                            // Command still running
                            if verbose > 0 {
                                println!("Command still running");
                            }
                        }
                        Err(e) => {
                            panic!("Failed to poll command process: {}", e);
                        }
                    }
                    thread_to_main_sender
                        .send(Message::CommandFinished)
                        .expect("Failed to send command process finished message");
                }
                Message::KillCommand => {
                    if command_process.is_none() {
                        panic!("Tried to kill a command while none was running");
                    }
                    // TODO: kill process gracefully
                    delete_command_process(command_process.unwrap());
                    command_process = None;
                    thread_to_main_sender
                        .send(Message::CommandKilled)
                        .expect("Failed to send command process finished message");
                }
                _ => panic!("Unexpected main to thread message"),
            } // End of match
        } // End of loop
    }) // End of thread
} // End of fn

fn create_command_process(args: &Vec<String>) -> std::process::Child {
    std::process::Command::new(&args[0])
        .args(&args[1..])
        .spawn()
        .expect("Failed to spawn command")
}

fn delete_command_process(mut command_process: std::process::Child) {
    command_process
        .kill()
        .expect("Failed to kill command process");
    // TODO: it is possible but a bit theoretical that the command process dies between test and kill
}
