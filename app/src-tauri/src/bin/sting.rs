//! Offline tuner for the Sting. Same code the app plays, rendered to a file.
//!
//!     cargo run --bin sting -- --out sting.wav
//!     cargo run --bin sting -- --out deep.wav --depth 1.8 --hit 2
//!     cargo run --bin sting -- --play --semitones -3

use threshold_app::audio::{play, render, write_wav, Params};

fn value(args: &[String], i: usize) -> Result<String, String> {
    args.get(i + 1)
        .cloned()
        .ok_or_else(|| format!("{} needs a value", args[i]))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p = Params::chosen();
    let mut out: Option<String> = Some("sting.wav".to_string());
    let mut do_play = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--semitones" => {
                p.semitones = value(&args, i)?.parse()?;
                i += 2;
            }
            "--t60" => {
                p.t60 = value(&args, i)?.parse()?;
                i += 2;
            }
            "--tail" => {
                p.tail = value(&args, i)?.parse()?;
                i += 2;
            }
            "--depth" => {
                p.depth = value(&args, i)?.parse()?;
                i += 2;
            }
            "--hit" => {
                p.hit = value(&args, i)?.parse()?;
                i += 2;
            }
            "--sr" => {
                p.sr = value(&args, i)?.parse()?;
                i += 2;
            }
            "--out" => {
                out = Some(value(&args, i)?);
                i += 2;
            }
            "--no-wav" => {
                out = None;
                i += 1;
            }
            "--play" => {
                do_play = true;
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "sting [--play] [--semitones f] [--depth f] [--hit 0|1|2] [--t60 f] [--tail f] [--sr f] [--out path] [--no-wav]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }

    // Keep the playback resampler and the renderer on the same safe rate.
    if !p.sr.is_finite() || !(8_000.0..=192_000.0).contains(&p.sr) {
        eprintln!("threshold: --sr must be between 8000 and 192000, using 48000");
        p.sr = 48_000.0;
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
