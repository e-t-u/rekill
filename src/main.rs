use clap::{Parser, command};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, default_value_t = false)]
    quiet: bool,

    #[arg(short, long, value_name = "SECONDS", default_value_t = 5)]
    time: u64,

    #[arg(short, long, default_value_t = false)]
    restart: bool,

    #[arg(value_name = "COMMAND", allow_hyphen_values = true, required = true)]
    command: Vec<String>,
}

// Messages between threads
enum Message {
    // Command thread to main
    CommandSender(std::sync::mpsc::Sender<Message>),
    CommandFinished, // Command died before timeout
    CommandRunning, // Command still running
    CommandKilled, // Confirm that the command was killed as requested
    // Timeout and poll threads to main
    Poll,
    TimeoutReached,
    // Main to thread
    StartCommand,
    PollCommand,
    KillCommand,
    // Restart is KillCommand + StartCommand
}


fn main() {
    let cli = Cli::parse();
    let command = cli.command;
    let verbose = cli.verbose;
    let quiet = cli.quiet;
    let time = cli.time;
    let restart = cli.restart;
    const POLL_TIME: u64 = 5;

    // Messages for the user
    // (Note: macros must be declared here where variables verbose and quiet are in scope)
    // Verbose levels: (how many -v options were given)
    // 0: no output
    // 1: info! timeout reached, command finished (stdout)
    // 2: verbose! + interpretation of arguments, polling, command finished (stderr)
    // 3: debug! + messages between threads (stderr)
    // TODO: Which should go to log instead of the terminal? Needed with future --daemon
    macro_rules! info {
        ($message:expr) => {
            if !quiet {
                println!($message);
            }
        };
        ($message:expr, $value:expr) => {
            if !quiet {
                println!($message, $value);
            }
        };
    }
    macro_rules! verbose {
        ($message:expr) => {
            if verbose >= 1 {
                eprintln!($message);
            }
        };
        ($message:expr, $value:expr) => {
            if verbose >= 1 {
                eprintln!($message, $value);
            }
        };
    }
    macro_rules! debug {
        ($message:expr) => {
            if verbose >= 2 {
                eprintln!($message);
            }
        };
        ($message:expr, $value:expr) => {
            if verbose >= 2 {
                eprintln!($message, $value);
            }
        };
    }

    verbose!("Command: {:?}", command);
    verbose!("Verbose: {}", verbose);
    verbose!("Time: {}", time);
    verbose!("Restart: {}", restart);
    verbose!("Poll time: {}", POLL_TIME);

    let (sender, thread_to_main_receiver) = std::sync::mpsc::channel::<Message>();
    let thread_to_main_sender = sender.clone();
    let timeout_to_main_sender = sender.clone();
    let poll_to_main_sender = sender.clone();

    // Start the command thread that controls the command process
    // Command means here the command that the user wants to run
    let _command_thread = std::thread::Builder::new()
        .name("command".to_string())
        .spawn(move ||  {
        debug!("The command thread started");
        // Function command_thread is a loop that does not return
        command_thread(command, verbose, thread_to_main_sender);
        std::unreachable!();  
    });

    // Start the timeout thread
    // When the timeout is reached, the main thread is woken up with message TimeoutReached
    let _timeout_thread = std::thread::Builder::new()
        .name("timeout".to_string())
        .spawn(move || {
        verbose!("Timeout thread started, timeout {} seconds", time);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(time));
            // Timeout thread sends TimeoutReached
            timeout_to_main_sender
                .send(Message::TimeoutReached)
                .expect("Failed to send timeout message");
            verbose!("TimeoutReached message sent");
        }
    });

    // Start the poll thread
    // The poll thread wakes up the main thread periodically
    // to check if the command is still running
    let _poll_thread = std::thread::Builder::new()
        .name("poll".to_string())
        .spawn(move || {
        verbose!("Poll thread started, poll time {} seconds", POLL_TIME);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(POLL_TIME));
            // Poll thread sends Poll
            poll_to_main_sender
                .send(Message::Poll)
                .expect("Failed to send poll command message");
            debug!("Polling message sent");
        }
    });

    // The channel from main to thread will be established when the command thread starts
    // and sends the endpoint using message CommandSender
    let mut main_to_thread_sender: Option<std::sync::mpsc::Sender<Message>> = None;
    
    // Ctrl-C handler
    let _ = ctrlc::set_handler(move || {
        verbose!("Ctrl-C received");
        debug!("Send KillCommand");
        // Send KillCommand
        // TODO: We could send KillCommand message to clean up the command process first
        // but it would require that we get the channel endpoint
        // that is difficult
        std::process::exit(0);
    });

    // The main thread's message receiver loop
    loop {
        match thread_to_main_receiver
            .recv()
            .expect("Failed to receive message from thread to main channel")
        {
            // The command thread starts and sends main_to_thread_sender endpoint
            // with message CommandSender
            Message::CommandSender(sender) => {
                main_to_thread_sender = Some(sender.clone());
                debug!("Command sender received");
                // Main sends StartCommand message to the command thread
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::StartCommand)
                        .expect("Failed to send start command message");
                    debug!("Sent StartCommand message")
                } else {
                    panic!("Command sender received but no channel established");
                }
            }

            // Main thread gets a poll message from the poll thread
            // Poll message is sent to the command thread to check
            // if the command is still running
            Message::Poll => {
                debug!("Received poll message");
                // Main sends PollCommand message to the command thread
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::PollCommand)
                        .expect("Failed to send poll command message");
                    debug!("Sent PollCommand message")
                } else {
                    panic!("Poll message received but no channel established");
                }
            }

            // Main thread gets a timeout message from the timeout thread
            // This tells that the command process should be restarted
            Message::TimeoutReached => {
                verbose!("Timeout reached");
                debug!("Send PollCommand to verify properly that the command is still running");
                // Main sends PollCommand message to the command thread
                // It may be that there is a long time from the last poll
                // and the command process is already dead
                // (note that this does not guarantee that the process does not die between poll and kill)
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::PollCommand)
                        .expect("Failed to send poll command message after timeout");
                    debug!("Sent PollCommand message")
                } else {
                    panic!("Sending PollCommand message received but no channel established");
                }
                verbose!("Killing command...");
                // Main sends KillCommand message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::KillCommand)
                        .expect("Failed to send kill command message");
                } else {
                    panic!("Main: Timeout message received but no channel established");
                }

            }

            // Main thread gets a command finished message from the command thread
            // This tells that the command process finished before the timeout
            // This is noticed as a result to PollCommand message
            Message::CommandFinished => {
                debug!("Command finished message received");
                if restart {
                    info!("Restarting...");
                    // Main sends StartCommand message
                    if let Some(t) = &main_to_thread_sender {
                        t.send(Message::StartCommand)
                            .expect("Failed to send start command message");
                    } else {
                        panic!("Command finished message received but no channel established");
                    }
                    debug!("Sent StartCommand message");
                    continue;
                } else {
                    verbose!("Exiting...");
                    info!("Command finished before restart timeout, if you want to restart it, add --restart");
                    // Subthreads are killed when the main thread exits
                    // TODO: return code could be the return code of the command process
                    std::process::exit(0);
                }
            }

            // Main thread get a command running message from the command thread
            // This is sent as a result to PollCommand message
            // when the command is still running OK
            // This is used just to notify the user if requested
            // (Command thread does not give verbose output to the user directly)
            Message::CommandRunning => {
                debug!("Command running message received");
                verbose!("Command still running OK");
            }

            // Main thread gets a command killed message from the command thread
            // This is confirmation that the command process was killed
            // as a result to KillCommand message
            Message::CommandKilled => {
                debug!("Command killed message received");
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
            _ => panic!("Main: Unexpected message received"),
        } // End of match
    } // End of loop
}

// The command thread starts and manages process that executes the user command given in arguments
fn command_thread(
    command: Vec<String>,
    verbose: u8,
    thread_to_main_sender: std::sync::mpsc::Sender<Message>,
) {
    // Macro must be again here where variable verbose is in the local scope
    // Thread reports only debug messages
    macro_rules! debug {
        ($message:expr) => {
            if verbose >= 2 {
                eprintln!(concat!("Command thread: ", $message));
            }
        };
        ($message:expr, $value:expr) => {
            if verbose >= 2 {
                eprintln!(concat!("Command thread: ", $message), $value);
            }
        };
    }
    // Thread creates a channel to receive messages from the main thread
    let (main_to_thread_sender, main_to_thread_receiver) =
        std::sync::mpsc::channel::<Message>();
    // Thread sends its receiving endpoint in the CommandSender message
    thread_to_main_sender
        .send(Message::CommandSender(main_to_thread_sender))
        .expect("Failed to send using thread to main sender");
    debug!("CommandSender sent");
    let mut command_process: Option<std::process::Child> = None;
    loop {
        match main_to_thread_receiver
            .recv()
            .expect("Failed to receive message from main to thread channel")
        {
            // The command thread's message receiver loop

            // Main thread sends StartCommand
            // Channel is established and we start the command process
            // This message is also sent in the restart after KillCommand
            Message::StartCommand => {
                debug!("StartCommand received");
                if let Some(_) = &mut command_process {
                    panic!("Tried to start a new command while another one was running");
                }
                let p = std::process::Command::new(&command[0])
                    .args(&command[1..])
                    .spawn()
                    .expect("Failed to spawn command");
                let id = p.id();
                command_process = Some(p);
                debug!("The process {} started OK", id);
            }

            // Main thread sends PollCommand
            // to ask to check if the command is still running
            Message::PollCommand => {
                debug!("PollCommand received");
                if let Some(p) = &mut command_process {
                    match p.try_wait() {
                        Ok(Some(status)) => {
                            // Command finished with status
                            debug!("Command {:?} finished OK", &command);
                            command_process = None;
                            // Send CommandFinished
                            thread_to_main_sender
                                .send(Message::CommandFinished)
                                .expect("Failed to send command process finished message");
                            debug!("Command process finished with status {:?}", status);
                            debug!("Sends CommandFinished")
                        }
                        Ok(None) => {
                            // Command still running
                            debug!("Command still running");
                            // Thread sends CommandRunning
                            thread_to_main_sender
                                .send(Message::CommandRunning)
                                .expect("Failed to send command process finished message");
                            debug!("Sends CommandRunning");
                        }
                        Err(e) => {
                            panic!("Failed to poll command process: {}", e);
                        }
                    }
                } else {
                    // Command process is not running
                    // This may happen if poll happens between KillCommand and StartCommand
                    // Or  if poll happens after (or during) KillCommand
                    // We just ignore this
                    debug!("Tried to poll a command process while none was running");
                }
            }

            // Main thread sends KillCommand
            // This means that the timeout is reached and the process must be killed
            Message::KillCommand => {
                debug!("KillCommand received");
                if let Some(mut p) = command_process {
                    debug!("Killing process {}", p.id());
                    p.kill().expect("Failed to kill command process");
                    // Kill leaves the process as zombie
                    // Wait receives return value and releases it
                    match p.wait() {
                        Ok(status) => debug!("Process exit OK, status {}", status),
                        Err(_) => panic!("Failed to reap command process"),
                    }
                    command_process = None;
                } else {
                    // We try to kill process that is already dead
                    // This may happen due to PollCommand (clean case)
                    // or the process may have died after the poll
                    debug!("Tried to kill a command while none was running");
                }
                // Thread sends CommandKilled
                thread_to_main_sender
                    .send(Message::CommandKilled)
                    .expect("Failed to send command process finished message");
                debug!("Sends CommandKilled")
            }

            _ => panic!("Unexpected main to thread message"),
        
        } // End of match
    } // End of loop
} // End of fn
