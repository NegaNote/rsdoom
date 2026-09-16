use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Warp {
    Map(u16),
    Episode(u16, u16),
}

#[derive(Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "This is a CLI argument struct with a lot of flags"
)]
pub struct CliArgs {
    pub iwad_path: PathBuf,
    pub pwad_paths: Vec<PathBuf>,
    pub nomonsters: bool,
    pub deathmatch: bool,
    pub fast: bool,         // fast monsters
    pub respawn: bool,      // respawn monsters
    pub warp: Option<Warp>, // warp to map
    pub skill: Option<u8>,  // skill level
}

const HELP: &str = r#"RSDoom - A Doom engine written in Rust
Usage: rsdoom [-iwad <path>] [-pwad <path1> <path2> ...] [-nomonsters] [-deathmatch] [-fast] [-warp <map>] [-respawn] [-skill <level>]
    -iwad <path>               Path to the IWAD file (defaults to "freedoom2.wad" in the current directory)
    -pwad <path1> <path2> ...  Paths to PWAD files (only supports one group of PWADs, will load in order)
    -warp <map>                Warp to the specified map (e.g., "01" for MAP01)
    -skill <level>             Set the skill level (1-5)
    -nomonsters                Disable monster spawning
    -deathmatch                Enable deathmatch mode
    -fast                      Enable fast monsters
    -respawn                   Enable monster respawning

Run with `--help` to see this message."#;

impl CliArgs {
    /// Parses command-line arguments and returns a `CliArgs` struct.
    /// Because doom's command-line arguments are single-dash but multi-character, we can't use clap, bpaf,
    /// or other argument parsing libraries that expect double-dash for multi-character arguments.
    /// Instead, we have to parse the arguments manually, draining as we go to be able to check for repeats
    /// or unknown arguments.
    /// # Errors
    /// Returns an error string if the arguments are invalid.
    pub fn parse_args(args: &mut Vec<String>) -> Result<Self, String> {
        fn get_flag(args: &mut Vec<String>, flag: &str) -> bool {
            args.iter().position(|arg| arg == flag).is_some_and(|pos| {
                args.remove(pos);
                true
            })
        }

        if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
            return Err(HELP.to_string());
        }

        let mut cli_args = Self::default();

        // Drain iwad and pwad arguments from the args vector
        if let Some(pos) = args.iter().position(|arg| arg == "-iwad") {
            args.remove(pos);
            let iwad_path = PathBuf::from(args.get(pos).ok_or("Missing path after -iwad")?);
            args.remove(pos);
            if !iwad_path.exists() {
                // We won't have to check the default at runtime since we're shipping freedoom2.wad with the engine
                // If it somehow doesn't exist anyway that's a problem with the engine distribution and not the user
                return Err(format!("IWAD file not found: {}", iwad_path.display()));
            }
            cli_args.iwad_path = iwad_path;
        }

        if let Some(pos) = args.iter().position(|arg| arg == "-pwad") {
            args.remove(pos);
            while let Some(arg) = args.get(pos)
                && !arg.starts_with('-')
            {
                let pwad_path = PathBuf::from(arg);
                if !pwad_path.exists() {
                    return Err(format!("PWAD file not found: {}", pwad_path.display()));
                }
                cli_args.pwad_paths.push(pwad_path);
                args.remove(pos);
            }

            if cli_args.pwad_paths.is_empty() {
                return Err("No PWAD files specified after -pwad".to_string());
            }
        }

        cli_args.nomonsters = get_flag(args, "-nomonsters");
        cli_args.deathmatch = get_flag(args, "-deathmatch");
        cli_args.fast = get_flag(args, "-fast");
        cli_args.respawn = get_flag(args, "-respawn");

        if let Some(pos) = args.iter().position(|arg| arg == "-warp") {
            args.remove(pos);
            let warp_arg1 = args
                .get(pos)
                .ok_or_else(|| "Missing map after -warp".to_string())?;
            let warp_arg1 = warp_arg1
                .parse::<u16>()
                .map_err(|_| format!("Invalid number after -warp: {warp_arg1}"))?;
            if warp_arg1 == 0 {
                return Err("Map number must be non-zero".to_string());
            }
            args.remove(pos);
            if let Some(s) = args.get(pos) {
                if let Ok(warp_arg2) = s.parse::<u16>() {
                    args.remove(pos);
                    if warp_arg2 == 0 {
                        return Err("Episode number must be non-zero".to_string());
                    }
                    cli_args.warp = Some(Warp::Episode(warp_arg1, warp_arg2));
                } else if s.starts_with('-') {
                    cli_args.warp = Some(Warp::Map(warp_arg1));
                } else {
                    return Err(format!("Invalid number after -warp: {s}"));
                }
            } else {
                cli_args.warp = Some(Warp::Map(warp_arg1));
            }
        }

        if let Some(pos) = args.iter().position(|arg| arg == "-skill") {
            args.remove(pos);
            let skill_arg = args
                .get(pos)
                .ok_or_else(|| "Missing skill level after -skill".to_string())?;
            let skill_level = skill_arg
                .parse::<u8>()
                .map_err(|_| format!("Invalid number after -skill: {skill_arg}"))?;
            if !(1..=5).contains(&skill_level) {
                return Err(format!(
                    "Skill level must be between 1 and 5, got: {skill_level}"
                ));
            }
            cli_args.skill = Some(skill_level);
            args.remove(pos);
        }

        if !args.is_empty() {
            // This should also catch duplicate uses of flags/arguments since we remove them from the args vector after processing
            return Err(format!("Unknown or extra arguments: {args:?}"));
        }

        Ok(cli_args)
    }
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            iwad_path: PathBuf::from("freedoom2.wad"),
            pwad_paths: Vec::new(),
            nomonsters: false,
            deathmatch: false,
            respawn: false,
            fast: false,
            warp: None,
            skill: None,
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn long_help_flag_gives_error() {
        let mut args = vec!["--help".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err(HELP.to_string()));
    }

    #[test]
    fn short_help_flag_gives_error() {
        let mut args = vec!["-h".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err(HELP.to_string()));
    }

    #[test]
    fn no_args_gives_default() {
        let mut args = Vec::new();
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Ok(CliArgs::default()));
    }

    #[test]
    fn unknown_args_gives_error() {
        let mut args = vec!["-unknown".to_string(), "arg".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-unknown\", \"arg\"]".to_string())
        );
    }

    #[test]
    fn all_flags_and_args_match() {
        let cargo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let first_pwad_path = cargo_dir.join("freedoom1.wad");
        let second_pwad_path = cargo_dir.join("freedoom2.wad");

        let mut args = vec![
            "-iwad".to_string(),
            "freedoom2.wad".to_string(),
            "-pwad".to_string(),
            first_pwad_path.display().to_string(),
            second_pwad_path.display().to_string(),
            "-nomonsters".to_string(),
            "-deathmatch".to_string(),
            "-fast".to_string(),
            "-respawn".to_string(),
            "-warp".to_string(),
            "1".to_string(),
            "2".to_string(),
            "-skill".to_string(),
            "3".to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Ok(CliArgs {
                iwad_path: PathBuf::from("freedoom2.wad"),
                pwad_paths: vec![first_pwad_path, second_pwad_path],
                nomonsters: true,
                deathmatch: true,
                fast: true,
                respawn: true,
                warp: Some(Warp::Episode(1, 2)),
                skill: Some(3),
            })
        );
    }

    #[test]
    fn args_can_be_in_any_order() {
        let cargo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let first_pwad_path = cargo_dir.join("freedoom1.wad");
        let second_pwad_path = cargo_dir.join("freedoom2.wad");

        let mut args = vec![
            "-nomonsters".to_string(),
            "-iwad".to_string(),
            "freedoom2.wad".to_string(),
            "-pwad".to_string(),
            first_pwad_path.display().to_string(),
            second_pwad_path.display().to_string(),
            "-warp".to_string(),
            "1".to_string(),
            "2".to_string(),
            "-skill".to_string(),
            "3".to_string(),
            "-deathmatch".to_string(),
            "-fast".to_string(),
            "-respawn".to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Ok(CliArgs {
                iwad_path: PathBuf::from("freedoom2.wad"),
                pwad_paths: vec![first_pwad_path, second_pwad_path],
                nomonsters: true,
                deathmatch: true,
                fast: true,
                respawn: true,
                warp: Some(Warp::Episode(1, 2)),
                skill: Some(3),
            })
        );
    }

    #[test]
    fn warp_map_only() {
        let mut args = vec!["-warp".to_string(), "1".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Ok(CliArgs {
                iwad_path: PathBuf::from("freedoom2.wad"),
                pwad_paths: vec![],
                nomonsters: false,
                deathmatch: false,
                fast: false,
                respawn: false,
                warp: Some(Warp::Map(1)),
                skill: None,
            })
        );
    }

    #[test]
    fn warp_map_with_small_number_gives_error() {
        let mut args = vec!["-warp".to_string(), "0".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Map number must be non-zero".to_string())
        );
    }

    #[test]
    fn warp_map_with_non_number_gives_error() {
        let mut args = vec!["-warp".to_string(), "abc".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err("Invalid number after -warp: abc".to_string()));
    }

    #[test]
    fn warp_episode_only() {
        let mut args = vec!["-warp".to_string(), "1".to_string(), "2".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Ok(CliArgs {
                iwad_path: PathBuf::from("freedoom2.wad"),
                pwad_paths: vec![],
                nomonsters: false,
                deathmatch: false,
                fast: false,
                respawn: false,
                warp: Some(Warp::Episode(1, 2)),
                skill: None,
            })
        );
    }

    #[test]
    fn warp_with_no_followup_gives_error() {
        let mut args = vec!["-warp".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err("Missing map after -warp".to_string()));
    }

    #[test]
    fn warp_followed_by_flag_gives_error() {
        let mut args = vec!["-warp".to_string(), "-nomonsters".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err("Missing map after -warp".to_string()));
    }

    #[test]
    fn warp_episode_with_non_number_gives_error() {
        let mut args = vec!["-warp".to_string(), "1".to_string(), "abc".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(cli_args, Err("Invalid number after -warp: abc".to_string()));
    }

    #[test]
    fn warp_episode_with_small_number_gives_error() {
        let mut args = vec!["-warp".to_string(), "1".to_string(), "0".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Episode number must be non-zero".to_string())
        );
    }

    #[test]
    fn pwad_without_files_gives_error() {
        let mut args = vec!["-pwad".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("No PWAD files specified after -pwad".to_string())
        );
    }

    #[test]
    fn pwad_with_nonexistent_file_gives_error() {
        let mut args = vec!["-pwad".to_string(), "nonexistent_file.wad".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("PWAD file not found: nonexistent_file.wad".to_string())
        );
    }

    #[test]
    fn pwad_followed_by_flag_gives_error() {
        let mut args = vec!["-pwad".to_string(), "-nomonsters".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("No PWAD files specified after -pwad".to_string())
        );
    }

    #[test]
    fn skill_with_invalid_number_gives_error() {
        let mut args = vec!["-skill".to_string(), "0".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Skill level must be between 1 and 5, got: 0".to_string())
        );
    }

    #[test]
    fn skill_with_non_number_gives_error() {
        let mut args = vec!["-skill".to_string(), "abc".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Invalid number after -skill: abc".to_string())
        );
    }

    #[test]
    fn skill_without_number_gives_error() {
        let mut args = vec!["-skill".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Missing skill level after -skill".to_string())
        );
    }

    #[test]
    fn skill_followed_by_flag_gives_error() {
        let mut args = vec!["-skill".to_string(), "-nomonsters".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Missing skill level after -skill".to_string())
        );
    }

    #[test]
    fn duplicate_skill_flag_gives_error() {
        let mut args = vec![
            "-skill".to_string(),
            "3".to_string(),
            "-skill".to_string(),
            "4".to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-skill\", \"4\"]".to_string())
        );
    }

    #[test]
    fn duplicate_warp_flag_gives_error() {
        let mut args = vec![
            "-warp".to_string(),
            "1".to_string(),
            "-warp".to_string(),
            "2".to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-warp\", \"2\"]".to_string())
        );
    }

    #[test]
    fn duplicate_pwad_flag_gives_error() {
        let cargo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let first_pwad_path = cargo_dir.join("freedoom1.wad");
        let whatever_pwad_path = cargo_dir.join("freedoom2.wad");
        let mut args = vec![
            "-pwad".to_string(),
            first_pwad_path.to_string_lossy().to_string(),
            "-pwad".to_string(),
            whatever_pwad_path.to_string_lossy().to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err(format!(
                "Unknown or extra arguments: [\"-pwad\", \"{}\"]",
                whatever_pwad_path.to_string_lossy()
            ))
        );
    }

    #[test]
    fn duplicate_iwad_flag_gives_error() {
        let mut args = vec![
            "-iwad".to_string(),
            "freedoom1.wad".to_string(),
            "-iwad".to_string(),
            "freedoom2.wad".to_string(),
        ];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-iwad\", \"freedoom2.wad\"]".to_string())
        );
    }

    #[test]
    fn duplicate_nomonsters_flag_gives_error() {
        let mut args = vec!["-nomonsters".to_string(), "-nomonsters".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-nomonsters\"]".to_string())
        );
    }

    #[test]
    fn duplicate_respawn_flag_gives_error() {
        let mut args = vec!["-respawn".to_string(), "-respawn".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-respawn\"]".to_string())
        );
    }

    #[test]
    fn duplicate_fast_flag_gives_error() {
        let mut args = vec!["-fast".to_string(), "-fast".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-fast\"]".to_string())
        );
    }

    #[test]
    fn duplicate_deathmatch_flag_gives_error() {
        let mut args = vec!["-deathmatch".to_string(), "-deathmatch".to_string()];
        let cli_args = CliArgs::parse_args(&mut args);
        assert_eq!(
            cli_args,
            Err("Unknown or extra arguments: [\"-deathmatch\"]".to_string())
        );
    }
}
