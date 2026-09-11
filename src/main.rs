use argparse::CliArgs;
use log::{error, info};
use std::process::ExitCode;

use rsdoom::argparse;
use sdl3::{event::Event, keyboard::Keycode, pixels::Color};
use simple_logger::SimpleLogger;
use std::{
    thread,
    time::{Duration, Instant},
};

const FRAME_TIME: Duration = Duration::new(0, 1_000_000_000u32 / 35);

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

    dbg!(&cli_args);

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

    info!("Created sdl3 window");

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
