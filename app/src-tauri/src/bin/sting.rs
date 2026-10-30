//! Offline tuner for the Sting. Same code the app plays, rendered to a file.
//!
//!     cargo run --bin sting -- --out sting.wav
//!     cargo run --bin sting -- --out deep.wav --depth 1.8 --hit 2
//!     cargo run --bin sting -- --play --semitones -3

use threshold_app::audio::{play, render, write_wav, Params};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p = Params::chosen();
    let mut out: Option<String> = Some("sting.wav".to_string());
    let mut do_play = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--semitones" => { p.semitones = args[i + 1].parse()?; i += 2; }
            "--t60" => { p.t60 = args[i + 1].parse()?; i += 2; }
            "--tail" => { p.tail = args[i + 1].parse()?; i += 2; }
            "--depth" => { p.depth = args[i + 1].parse()?; i += 2; }
            "--hit" => { p.hit = args[i + 1].parse()?; i += 2; }
            "--sr" => { p.sr = args[i + 1].parse()?; i += 2; }
            "--out" => { out = Some(args[i + 1].clone()); i += 2; }
            "--no-wav" => { out = None; i += 1; }
            "--play" => { do_play = true; i += 1; }
            "-h" | "--help" => {
                println!(
                    "sting [--play] [--semitones f] [--depth f] [--hit 0|1|2] [--t60 f] [--tail f] [--sr f] [--out path] [--no-wav]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }

    let sting = render(&p);
    println!(
        "rendered {:.2}s @ {} Hz · peak {:.2} dBFS · semitones {:+.1} · t60 {:.2}s · depth {:.2} · hit {}",
        sting.stereo.len() as f32 / p.sr,
        p.sr,
        20.0 * sting.peak.log10(),
        p.semitones,
        p.t60,
        p.depth,
        p.hit
    );

    if let Some(path) = out {
        write_wav(&path, &sting.stereo, p.sr)?;
        println!("wrote {path}");
    }
    if do_play {
        play(sting.stereo, p.sr)?;
    }
    Ok(())
}
