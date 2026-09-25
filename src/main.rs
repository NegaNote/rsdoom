use argparse::CliArgs;
use log::{debug, error, info};
use std::process::ExitCode;

use hashbrown::HashMap;
use rsdoom::argparse;
use rsdoom::wad::assets::gfx::{GfxAsset, parse_pnames, parse_vanilla_texture_definitions};
use rsdoom::wad::raw::{LumpName, Namespace, WadType, load_wad, patch_wad};
use sdl3::{event::Event, keyboard::Keycode, pixels::Color};
use simple_logger::SimpleLogger;
use std::{
    thread,
    time::{Duration, Instant},
};

const FRAME_TIME: Duration = Duration::new(0, 1_000_000_000u32 / 35);

#[expect(
    clippy::too_many_lines,
    reason = "This is the main function, which is expected to be long."
)]
fn main() -> ExitCode {
    let logging_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let Ok(()) = SimpleLogger::new()
        .with_level(logging_level)
        .env()
        .with_threads(true)
        .with_local_timestamps()
        .init()
    else {
        eprintln!("Failed to initialize logger");
        return ExitCode::from(1);
    };

    let cli_args = match CliArgs::parse_args(
        &mut std::env::args_os()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
    ) {
        Ok(args) => args,
        Err(err) => {
            // Intentionally using eprintln! instead of logging because we want to directly print out
            // the error messages, which could include the usage text,
            // without any additional formatting or prefixes that logging might add.
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    info!(
        "Starting RSDoom with IWAD: {} and PWADs: {:?}",
        cli_args.iwad_path.display(),
        cli_args
            .pwad_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    );

    let wad_result = load_wad(&cli_args.iwad_path, WadType::Iwad);
    let mut wad = match wad_result {
        Ok(wad) => {
            info!("Loaded IWAD file: {}", cli_args.iwad_path.display());
            wad
        }
        Err(err) => {
            error!("Failed to load WAD file: {err}");
            return ExitCode::FAILURE;
        }
    };

    for pwad_path in &cli_args.pwad_paths {
        match load_wad(pwad_path, WadType::Pwad) {
            Ok(pwad) => {
                info!("Loaded PWAD file: {}", pwad_path.display());
                patch_wad(&mut wad, &pwad);
            }
            Err(err) => {
                error!("Failed to load PWAD file {}: {err}", pwad_path.display());
                return ExitCode::FAILURE;
            }
        }
    }

    let mut gfx_asset_map: HashMap<LumpName, GfxAsset> = HashMap::new();

    let hardcoded_lump_names = [
        "TITLEPIC", "DMENUPIC", "HELP", "HELP1", "HELP2", "CREDIT", "VICTORY1", "VICTORY2",
        "ENDPIC", "PAUSED", "ADVISOR", "M_DOOM", "M_EPISOD", "M_NEWG", "M_SKILL", "M_OPTION",
        "M_LOADG", "M_SAVEG", "M_LSLEFT", "M_LSCNTR", "M_LSRGHT", "M_OPTTTL", "M_SVOL", "M_VBOX",
        "M_COLORS", "M_PALSEL", "M_THERML", "M_THERMM", "M_THERMR", "M_THERMO", "M_SKULL1",
        "M_SKULL2", "M_FSLOT", "PFUB1", "PFUB2", "END0", "END1", "END2", "END3", "END4", "END5",
        "END6", "BOSSBACK", "STBAR", "brdr_b", "STARMS", "STTNUM0", "STTNUM1", "STTNUM2",
        "STTNUM3", "STTNUM4", "STTNUM5", "STTNUM6", "STTNUM7", "STTNUM8", "STTNUM9", "STYSNUM0",
        "STYSNUM1", "STYSNUM2", "STYSNUM3", "STYSNUM4", "STYSNUM5", "STYSNUM6", "STYSNUM7",
        "STYSNUM8", "STYSNUM9", "STTPRCNT", "STKEYS0", "STKEYS1", "STKEYS2", "STKEYS3", "STKEYS4",
        "STKEYS5", "STKEYS6", "STKEYS7", "STGNUM2", "STGNUM3", "STGNUM4", "STGNUM5", "STGNUM6",
        "STGNUM7", "STFB0", "STFST00", "STFST01", "STFST02", "STFST03", "STFST04", "STFST05",
        "STFST06", "STFST07", "STFST08", "STFST09", "STFST10", "STFST11", "STFST12", "STFST13",
        "STFST14", "STFST15", "STFST16", "STFST17", "STFST18", "STFST19", "STFST20", "STFST21",
        "STFST22", "STFST23", "STFST24", "STFST25", "STFST26", "STFST27", "STFST28", "STFST29",
        "STFST30", "STFST31", "STFST32", "STFST33", "STFTR00", "STFTR01", "STFTR02", "STFTR03",
        "STFTR04", "STFTR05", "STFTR06", "STFTR07", "STFTR08", "STFTR09", "STFTR10", "STFTR11",
        "STFTR12", "STFTR13", "STFTR14", "STFTR15", "STFTR16", "STFTR17", "STFTR18", "STFTR19",
        "STFTR20", "STFTR21", "STFTR22", "STFTR23", "STFTR24", "STFTR25", "STFTR26", "STFTR27",
        "STFTR28", "STFTR29", "STFTR30", "STFTL00", "STFTL01", "STFTL02", "STFTL03", "STFTL04",
        "STFTL05", "STFTL06", "STFTL07", "STFTL08", "STFTL09", "STFTL10", "STFTL11", "STFTL12",
        "STFTL13", "STFTL14", "STFTL15", "STFTL16", "STFTL17", "STFTL18", "STFTL19", "STFTL20",
        "STFTL21", "STFTL22", "STFTL23", "STFTL24", "STFTL25", "STFTL26", "STFTL27", "STFTL28",
        "STFTL29", "STFTL30", "STFOUCH0", "STFOUCH1", "STFOUCH2", "STFOUCH3", "STFOUCH4",
        "STFEVL0", "STFEVL1", "STFEVL2", "STFEVL3", "STFEVL4", "STFKILL0", "STFKILL1", "STFKILL2",
        "STFKILL3", "STFKILL4", "STFGOD0", "STFDEAD0", "STAR",
    ];

    for lump in
        wad.get_lumps_by_namespace(Namespace::Sprites)
            .chain(wad.get_lumps_by_namespace(Namespace::HiRes))
            .chain(hardcoded_lump_names.iter().filter_map(|&name| {
                wad.get_lump_by_str_name_and_namespace(name, Namespace::Global)
            }))
    {
        let asset: GfxAsset = match GfxAsset::new(lump.get_raw_data()) {
            Ok(asset) => asset,
            Err(err) => {
                error!(
                    "Failed to parse sprite asset from lump {}: {err}",
                    lump.get_name()
                );
                return ExitCode::FAILURE;
            }
        };

        gfx_asset_map.insert(lump.get_name(), asset);
        debug!("Parsed sprite asset from lump {}", lump.get_name());
    }

    info!("Parsed {} sprite assets", gfx_asset_map.len());

    let Some(pnames) = wad.get_lump_by_str_name_and_namespace("PNAMES", Namespace::Global) else {
        error!("Could not find PNAMES lump");
        return ExitCode::FAILURE;
    };

    let patch_names = match parse_pnames(pnames.get_raw_data()) {
        Ok(pnames) => {
            debug!("Parsed PNAMES lump with {} entries", pnames.len());
            pnames
        }
        Err(err) => {
            error!("Failed to parse PNAMES lump: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut texture_definitions = if let Some(texture_lump) =
        wad.get_lump_by_str_name_and_namespace("TEXTURE1", Namespace::Global)
    {
        match parse_vanilla_texture_definitions(texture_lump.get_raw_data()) {
            Ok(defs) => {
                debug!("Parsed TEXTURE1 lump with {} entries", defs.len());
                defs
            }
            Err(err) => {
                error!("Failed to parse TEXTURE1 lump: {err}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        error!("Could not find TEXTURE1 lump");
        return ExitCode::FAILURE;
    };

    if texture_definitions.is_empty() {
        error!("No texture definitions found in TEXTURE1 lump");
        return ExitCode::FAILURE;
    }

    if let Some(texture_lump) =
        wad.get_lump_by_str_name_and_namespace("TEXTURE2", Namespace::Global)
    {
        match parse_vanilla_texture_definitions(texture_lump.get_raw_data()) {
            Ok(defs) => {
                debug!("Parsed TEXTURE2 lump with {} entries", defs.len());
                // Append the definitions from TEXTURE2 to the existing definitions
                texture_definitions.extend(defs);
            }
            Err(err) => {
                error!("Failed to parse TEXTURE2 lump: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    info!(
        "Parsed PNAMES, TEXTURE1, and TEXTURE2 lumps with a total of {} texture definitions",
        texture_definitions.len()
    );

    let Ok(sdl_context) = sdl3::init() else {
        error!("Could not initialize SDL3");
        return ExitCode::FAILURE;
    };

    let Ok(video_subsystem) = sdl_context.video() else {
        error!("Could not initialize video subsystem");
        return ExitCode::FAILURE;
    };

    let Ok(window) = video_subsystem
        .window("RSDoom", 640, 480)
        .position_centered()
        .build()
    else {
        error!("Could not create window");
        return ExitCode::FAILURE;
    };

    let mut canvas = window.into_canvas();

    canvas.set_draw_color(Color::RGB(0, 255, 255));
    canvas.clear();
    canvas.present();

    debug!("Created sdl3 window");

    // This is just placeholder code, will implement actual game logic later

    let Ok(mut event_pump) = sdl_context.event_pump() else {
        error!("Could not create event pump");
        return ExitCode::FAILURE;
    };
    let mut i: u8 = 0;
    'running: loop {
        let frame_start = Instant::now();
        i = i.wrapping_add(1);
        canvas.set_draw_color(Color::RGB(i, 64, 255_u8.wrapping_sub(i)));
        canvas.clear();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        canvas.present();

        let elapsed = frame_start.elapsed();

        FRAME_TIME
            .checked_sub(elapsed)
            .inspect(|dur| thread::sleep(*dur));
    }

    info!("Exiting RSDoom");

    ExitCode::SUCCESS
}
