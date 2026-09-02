//! Seed data: command families with structured options + known tools.

/// One command option/flag with its description.
#[derive(Clone, Copy)]
pub struct SeedOption {
    pub flag: &'static str,
    pub description: &'static str,
}

/// A command family: the command itself plus its structured options.
#[derive(Clone, Copy)]
pub struct SeedCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub options: &'static [SeedOption],
}

/// Known tools worth having as `app` entities (desktop use, terminal only).
pub const SEED_TOOLS: &[(&str, &str, &str)] = &[
    // name, description, tag
    ("yazi", "Blazing fast terminal file browser", "file-manager"),
    (
        "ranger",
        "Console file manager with VI key bindings",
        "file-manager",
    ),
    (
        "lf",
        "Terminal file manager (Go, ranger-like)",
        "file-manager",
    ),
    (
        "btop",
        "Resource monitor with CPU/memory/network graphs",
        "process-viewer",
    ),
    ("htop", "Interactive process viewer", "process-viewer"),
    (
        "fastfetch",
        "System information tool (neofetch successor)",
        "fetch",
    ),
    (
        "neofetch",
        "System information tool with ASCII logo",
        "fetch",
    ),
    (
        "nvim",
        "Neovim — hyperextensible Vim-based text editor",
        "editor",
    ),
    ("vim", "Vi IMproved — terminal text editor", "editor"),
    ("nano", "Small, friendly terminal text editor", "editor"),
    ("lazygit", "Simple terminal UI for git commands", "git"),
    (
        "lazydocker",
        "The lazier way to manage everything docker",
        "docker",
    ),
    (
        "k9s",
        "Terminal UI to interact with your Kubernetes clusters",
        "kubernetes",
    ),
    ("ctop", "Top-like interface for container metrics", "docker"),
    ("fzf", "Command-line fuzzy finder", "fuzzy-finder"),
    (
        "kubectl",
        "Command-line tool for controlling Kubernetes clusters",
        "kubernetes",
    ),
    (
        "docker compose",
        "Define and run multi-container applications",
        "docker",
    ),
];

/// Command families seeded into the knowledge base (part 1).
pub const SEED_COMMANDS_A: &[SeedCommand] = &[
    SeedCommand {
        name: "git",
        description: "Distributed version control system",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show the working tree status",
            },
            SeedOption {
                flag: "add",
                description: "Add file contents to the staging area",
            },
            SeedOption {
                flag: "commit",
                description: "Record changes to the repository",
            },
            SeedOption {
                flag: "push",
                description: "Update remote refs with local commits",
            },
            SeedOption {
                flag: "pull",
                description: "Fetch from and integrate with a remote",
            },
            SeedOption {
                flag: "log",
                description: "Show commit history",
            },
            SeedOption {
                flag: "diff",
                description: "Show changes between commits/working tree",
            },
            SeedOption {
                flag: "branch",
                description: "List, create or delete branches",
            },
        ],
    },
    SeedCommand {
        name: "docker",
        description: "Container engine — build, ship and run containers",
        options: &[
            SeedOption {
                flag: "ps",
                description: "List containers",
            },
            SeedOption {
                flag: "images",
                description: "List images",
            },
            SeedOption {
                flag: "run",
                description: "Run a command in a new container",
            },
            SeedOption {
                flag: "build",
                description: "Build an image from a Dockerfile",
            },
            SeedOption {
                flag: "pull",
                description: "Pull an image from a registry",
            },
            SeedOption {
                flag: "logs",
                description: "Fetch container logs",
            },
            SeedOption {
                flag: "exec",
                description: "Run a command inside a running container",
            },
        ],
    },
    SeedCommand {
        name: "systemctl",
        description: "Control the systemd system and service manager",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show unit status",
            },
            SeedOption {
                flag: "start",
                description: "Start a unit",
            },
            SeedOption {
                flag: "stop",
                description: "Stop a unit",
            },
            SeedOption {
                flag: "restart",
                description: "Restart a unit",
            },
            SeedOption {
                flag: "enable",
                description: "Enable a unit to start at boot",
            },
            SeedOption {
                flag: "disable",
                description: "Disable a unit from starting at boot",
            },
            SeedOption {
                flag: "list-units",
                description: "List loaded units",
            },
        ],
    },
    SeedCommand {
        name: "rc-service",
        description: "OpenRC service manager — start/stop services",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show service status",
            },
            SeedOption {
                flag: "start",
                description: "Start a service",
            },
            SeedOption {
                flag: "stop",
                description: "Stop a service",
            },
            SeedOption {
                flag: "restart",
                description: "Restart a service",
            },
        ],
    },
    SeedCommand {
        name: "rc-update",
        description: "OpenRC — manage services that run at boot",
        options: &[
            SeedOption {
                flag: "add",
                description: "Add a service to a runlevel",
            },
            SeedOption {
                flag: "delete",
                description: "Remove a service from a runlevel",
            },
            SeedOption {
                flag: "show",
                description: "Show services in runlevels",
            },
        ],
    },
    SeedCommand {
        name: "journalctl",
        description: "Query the systemd journal",
        options: &[
            SeedOption {
                flag: "-f",
                description: "Follow new journal messages",
            },
            SeedOption {
                flag: "-u",
                description: "Show messages of a unit",
            },
            SeedOption {
                flag: "--since",
                description: "Show entries since a date/time",
            },
            SeedOption {
                flag: "-b",
                description: "Show messages from a boot",
            },
        ],
    },
    SeedCommand {
        name: "ssh",
        description: "OpenSSH remote login client",
        options: &[
            SeedOption {
                flag: "-p",
                description: "Connect to a specific port",
            },
            SeedOption {
                flag: "-i",
                description: "Select the identity (private key) file",
            },
            SeedOption {
                flag: "-v",
                description: "Verbose mode (debug output)",
            },
            SeedOption {
                flag: "-L",
                description: "Local port forwarding",
            },
        ],
    },
];

/// Command families seeded into the knowledge base (part 2).
pub const SEED_COMMANDS_B: &[SeedCommand] = &[
    SeedCommand {
        name: "curl",
        description: "Transfer data to/from URLs",
        options: &[
            SeedOption {
                flag: "-X",
                description: "HTTP request method",
            },
            SeedOption {
                flag: "-H",
                description: "Add a request header",
            },
            SeedOption {
                flag: "-d",
                description: "Send request body data",
            },
            SeedOption {
                flag: "-o",
                description: "Write output to a file",
            },
            SeedOption {
                flag: "-L",
                description: "Follow redirects",
            },
            SeedOption {
                flag: "-I",
                description: "Fetch headers only",
            },
        ],
    },
    SeedCommand {
        name: "grep",
        description: "Print lines matching a pattern",
        options: &[
            SeedOption {
                flag: "-r",
                description: "Search directories recursively",
            },
            SeedOption {
                flag: "-i",
                description: "Ignore case",
            },
            SeedOption {
                flag: "-n",
                description: "Show line numbers",
            },
            SeedOption {
                flag: "-v",
                description: "Invert match",
            },
            SeedOption {
                flag: "-E",
                description: "Extended regular expressions",
            },
        ],
    },
    SeedCommand {
        name: "find",
        description: "Search for files in a directory hierarchy",
        options: &[
            SeedOption {
                flag: "-name",
                description: "Match file name (glob)",
            },
            SeedOption {
                flag: "-type",
                description: "Match file type (f=file, d=dir)",
            },
            SeedOption {
                flag: "-size",
                description: "Match file size",
            },
            SeedOption {
                flag: "-exec",
                description: "Run a command on each match",
            },
        ],
    },
    SeedCommand {
        name: "tar",
        description: "Archive files (tarball utility)",
        options: &[
            SeedOption {
                flag: "-c",
                description: "Create an archive",
            },
            SeedOption {
                flag: "-x",
                description: "Extract an archive",
            },
            SeedOption {
                flag: "-z",
                description: "Filter through gzip",
            },
            SeedOption {
                flag: "-f",
                description: "Use the given archive file",
            },
            SeedOption {
                flag: "-v",
                description: "Verbose output",
            },
        ],
    },
    SeedCommand {
        name: "python3",
        description: "Python interpreter",
        options: &[
            SeedOption {
                flag: "-m",
                description: "Run a module as a script",
            },
            SeedOption {
                flag: "-V",
                description: "Print the version",
            },
            SeedOption {
                flag: "-c",
                description: "Execute the given program text",
            },
            SeedOption {
                flag: "-i",
                description: "Interactive REPL after running script",
            },
        ],
    },
    SeedCommand {
        name: "cargo",
        description: "Rust package manager and build tool",
        options: &[
            SeedOption {
                flag: "build",
                description: "Compile the current package",
            },
            SeedOption {
                flag: "run",
                description: "Build and run the binary target",
            },
            SeedOption {
                flag: "test",
                description: "Run tests",
            },
            SeedOption {
                flag: "check",
                description: "Fast type-check without codegen",
            },
            SeedOption {
                flag: "add",
                description: "Add a dependency",
            },
            SeedOption {
                flag: "fmt",
                description: "Format code with rustfmt",
            },
        ],
    },
];

/// All command families (parts combined).
pub fn seed_commands() -> impl Iterator<Item = &'static SeedCommand> {
    SEED_COMMANDS_A.iter().chain(SEED_COMMANDS_B.iter())
}
