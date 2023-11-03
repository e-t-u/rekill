use clap::{command, Parser};

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
    Endpoint(std::sync::mpsc::Sender<Message>),
    Finished, // Command died before timeout
    Running,  // Command still running
    Killed,   // Confirm that the command was killed as requested
    // Main to command thread
    Start, // Start the command process
    Poll, // Check if the command is still running
    Kill, // Kill the command process
    // Restart is KillCommand + StartCommand
    // Ctrl-C handler to main
    CtrlC,
}

fn main() {
    let cli = Cli::parse();
    let command = cli.command;
    let verbose = cli.verbose;
    let quiet = cli.quiet;
    let time = cli.time;
    let restart = cli.restart;
    const POLL_TIME: u64 = 500; // milliseconds

    // Messages for the user
    // (Note: macros must be declared here where variables verbose and quiet are in scope)
    // Verbose levels: (how many -v options were given)
    // quiet: no output from info!
    // 0: info! timeout reached, command finished (stdout)
    // 1: verbose! + interpretation of arguments, polling, command finished (stderr)
    // 2: debug! + messages between threads (stderr)
    // TODO: Wchich of these should go to log file instead? Needed with future --daemon
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
    verbose!("Time: {} seconds", time);
    verbose!("Restart: {}", restart);
    verbose!("Poll time: {} milliseconds", POLL_TIME);

    let (sender, thread_to_main_receiver) = std::sync::mpsc::channel::<Message>();
    let thread_to_main_sender = sender.clone();
    let ctrlc_to_main_sender = sender.clone();

    // Start the command thread that controls the process that executes the user command
    let _command_thread = std::thread::Builder::new()
        .name("command".to_string()) // This is shown in panic! messages
        .spawn(move || {
            debug!("The command thread started");
            // Function command_thread is a loop that does not return
            command_thread(command, verbose, thread_to_main_sender);
            std::unreachable!();
        });

    // The channel from main to command thread will be established when the command thread starts
    // and sends the endpoint using message CommandSender
    let mut main_to_thread_sender: Option<std::sync::mpsc::Sender<Message>> = None;

    // Ctrl-C handler
    let _ = ctrlc::set_handler(move || {
        info!("...Ctrl-C");
        debug!("Send CtrlC message to the main thread");
        // Send CtrlC message to the main thread
        ctrlc_to_main_sender
            .send(Message::CtrlC)
            .expect("Failed to send CtrlC message");
    });

    // Set initial timeout time
    let mut timeout = std::time::Instant::now() + std::time::Duration::from_secs(time);

    // The main thread's message receiver loop
    loop {
        match thread_to_main_receiver
            .recv_timeout(std::time::Duration::from_millis(POLL_TIME))
            // .expect("Failed to receive message from thread to main channel")
        {
            // The command thread starts and sends main_to_thread_sender endpoint
            // with message CommandSender
            Ok(Message::Endpoint(sender)) => {
                main_to_thread_sender = Some(sender.clone());
                debug!("Main: Endpoint message received");
                // Main sends StartCommand message to the command thread
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::Start)
                        .expect("Failed to send start command message");
                    debug!("Main: Sent Start message")
                } else {
                    panic!("Command sender received but no channel established");
                }
            }


            // Main thread gets a poll message from the poll thread
            // Poll message is sent to the command thread to check
            // if the command is still running
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                debug!("Main: Message loop timeout");
                // Check if timeout is reached
                if std::time::Instant::now() > timeout {
                    // Timeout reached
                    info!("Timeout reached");
                    // Main sends Kill message
                    if let Some(t) = &main_to_thread_sender {
                        t.send(Message::Kill)
                            .expect("Failed to send kill command message");
                        debug!("Main: Sent Kill message");
                    } else {
                        panic!("Timeout message received but no channel established");
                    }
                    // Start message will be sent by Killed message handler
                    // Set new timeout time
                    timeout = std::time::Instant::now() + std::time::Duration::from_secs(time);
                } else {
                    // Timeout not reached
                    // Main sends Poll message to the command thread to check if the command is alive
                    if let Some(t) = &main_to_thread_sender {
                        t.send(Message::Poll)
                            .expect("Failed to send poll message");
                        debug!("Main: Sent Poll message")
                    } else {
                        debug!("Main: Trying to send Poll message before the channel was established");
                    }
                }
            }

            // Main thread gets a command finished message from the command thread
            // This is a respond to PollCommand message
            // This tells that the command process finished before the timeout
            Ok(Message::Finished) => {
                debug!("Main: Command finished message received");
                if restart {
                    // BUG: Here is a bug! The timeout timer must start from 
                    // the zero again when the command is restarted
                    // This requires that the timeout thread is either restarted
                    // or it receives a message to reset the timer
                    // It requires a whone new channel
                    info!("Command finished before timeout. Restarting.");
                    // Main sends Start message
                    if let Some(t) = &main_to_thread_sender {
                        t.send(Message::Start)
                            .expect("Failed to send start command message");
                        debug!("Main: Sent Start message");
                    } else {
                        panic!("Command finished message received but no channel established");
                    }
                    continue;
                } else {
                    verbose!("Exiting...");
                    info!("Command finished before restart timeout, if you want to restart it, add --restart");
                    // Subthreads are killed when the main thread exits
                    // TODO: return code could be the return code of the command process
                    std::process::exit(0);
                }
            }

            // Main thread gets a CtrlC message from the Ctrl-C handler
            Ok(Message::CtrlC) => {
                debug!("Main: CtrlC message received");
                debug!("Main: Sending KillCommand message to the command thread");
                // Main sends Kill message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::Kill)
                        .expect("Failed to send kill command message (ctrl-C)");
                    debug!("Main: Sent Kill message");
                } else {
                    // Ctrl-C may happen before the command thread channel is established
                    debug!("Main: CtrlC message received but no channel established");
                }
                // Exit must wait for the command to be killed but we should duplicate the kill command
                // to kill command and die. Therefore, we just simply give the command thread and process
                // 0.1 seconds to clean up and then main thread exits and kills all subthreads as well
                std::thread::sleep(std::time::Duration::from_millis(100));
                std::process::exit(0);
            }

            // Main thread get a command running message from the command thread
            // This is sent as a result to PollCommand message
            // when the command is still running OK
            // This is used just to notify the user if requested
            // (Command thread does not give verbose output to the user directly)
            Ok(Message::Running) => {
                debug!("Main: Running message received");
                verbose!("Command still running OK");
            }

            // Main thread gets a command killed message from the command thread
            // This is confirmation that the command process was killed
            // as a result to KillCommand message
            Ok(Message::Killed) => {
                debug!("Main: Killed message received");
                verbose!("Command killed because of the timeout");
                // Main sends Start message
                if let Some(t) = &main_to_thread_sender {
                    t.send(Message::Start)
                        .expect("Failed to send start command message");
                    debug!("Main: Sent Start message");
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
    let (main_to_thread_sender, main_to_thread_receiver) = std::sync::mpsc::channel::<Message>();
    // Thread sends its receiving endpoint in the Endpoint message
    thread_to_main_sender
        .send(Message::Endpoint(main_to_thread_sender))
        .expect("Failed to send Endpoint message");
    debug!("Endpoint message sent");
    let mut command_process: Option<std::process::Child> = None;
    loop {
        match main_to_thread_receiver
            .recv()
            .expect("Failed to receive message from main to thread channel")
        {
            // The command thread's message receiver loop

            // Main thread sends Start message
            // Channel is established and we start the command process
            // This message is also sent in the restart after KillCommand
            Message::Start => {
                debug!("Start message received");
                if let Some(_) = &mut command_process {
                    panic!("Tried to start a new command while another one was running");
                }
                let p = std::process::Command::new(&command[0])
                    .args(&command[1..])
                    .spawn()
                    .expect(&format!("Failed to spawn command {:?}", &command));
                let id = p.id();
                command_process = Some(p);
                debug!("The process {} started OK", id);
            }

            // Main thread sends PollCommand
            // to ask to check if the command is still running
            Message::Poll => {
                debug!("Poll message received");
                if let Some(p) = &mut command_process {
                    match p.try_wait() {
                        Ok(Some(status)) => {
                            // Command finished with status
                            debug!("Command {:?} finished OK", &command);
                            command_process = None;
                            // Send CommandFinished
                            thread_to_main_sender
                                .send(Message::Finished)
                                .expect("Failed to send command process finished message");
                            debug!("Command process finished with status {:?}", status);
                            debug!("Finished message sent")
                        }
                        Ok(None) => {
                            // Command still running
                            debug!("Command still running");
                            // Thread sends CommandRunning
                            thread_to_main_sender
                                .send(Message::Running)
                                .expect("Failed to send command process finished message");
                            debug!("Running message sent");
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

            // Main thread sends Kill message
            // This means that the timeout is reached and the process must be killed
            Message::Kill => {
                debug!("Kill message received");
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
                    .send(Message::Killed)
                    .expect("Failed to send command process finished message");
                debug!("Killed message sent")
            }

            _ => panic!("Unexpected main to thread message"),
        } // End of match
    } // End of loop
} // End of fn
