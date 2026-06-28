//! BCPL `Sound_*` and `Music_*` runtime — **MacBCPL port stub.**
//!
//! The original NewBCPL `audio` module renders preset SFX and ABC
//! tunes through the external `newaudio-core` / `newaudio-abc` crates
//! (and plays them live via `newaudio-win` on Windows). Those crates
//! are out of scope for the macOS arm64 port, so this module is a
//! self-contained stub that keeps the **exact** C-ABI symbol surface
//! `builtins.rs` registers — every `Sound_*` and `Music_*` function,
//! same name and signature — but performs only lightweight slot
//! bookkeeping and makes no sound.
//!
//! This keeps the JIT symbol table and the cross-platform tests happy.
//! When MacBCPL grows real audio it should go through the Cocoa bridge
//! (AVAudioEngine), the same path MacModula2 uses for its `Sfx` layer.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// ─── status / waveform constants (unchanged from upstream) ──────────

pub const MUSIC_STATE_STOPPED: i64 = 0;
pub const MUSIC_STATE_PLAYING: i64 = 1;
pub const MUSIC_STATE_PAUSED: i64 = 2;

pub const BCPL_AUDIO_OK: i64 = 0;
pub const BCPL_AUDIO_ERR_PARSE: i64 = 1;
pub const BCPL_AUDIO_ERR_UNKNOWN_SLOT: i64 = 2;
pub const BCPL_AUDIO_ERR_NO_DEVICE: i64 = 3;

// ─── minimal slot bookkeeping ───────────────────────────────────────

#[derive(Default)]
struct AudioState {
    /// slot -> nominal duration (seconds) of the "rendered" sound.
    sounds: HashMap<i64, f64>,
    /// music slot -> tempo (bpm); presence means "loaded".
    music: HashMap<i64, f64>,
    sound_volume: f64,
    music_volume: f64,
    music_state: i64,
}

fn with_state<R>(f: impl FnOnce(&mut AudioState) -> R) -> R {
    static STATE: OnceLock<Mutex<AudioState>> = OnceLock::new();
    let mu = STATE.get_or_init(|| {
        Mutex::new(AudioState {
            sound_volume: 1.0,
            music_volume: 1.0,
            music_state: MUSIC_STATE_STOPPED,
            ..Default::default()
        })
    });
    let mut g = mu.lock().expect("audio state mutex poisoned");
    f(&mut g)
}

/// Register a "sound" of `duration` seconds in `slot`.
fn store_sound(slot: i64, duration: f64) -> i64 {
    with_state(|s| {
        s.sounds.insert(slot, duration.max(0.0));
    });
    BCPL_AUDIO_OK
}

// ─── Sound presets — store the requested duration, no synthesis ─────

macro_rules! sound_preset {
    ($name:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C-unwind" fn $name(slot: i64, _p1: f64, duration: f64) -> i64 {
            store_sound(slot, duration)
        }
    };
}

sound_preset!(Sound_Beep);
sound_preset!(Sound_Coin);
sound_preset!(Sound_Jump);
sound_preset!(Sound_Explode);
sound_preset!(Sound_BigExplode);
sound_preset!(Sound_SmallExplode);
sound_preset!(Sound_DistantExplode);
sound_preset!(Sound_MetalExplode);
sound_preset!(Sound_Zap);
sound_preset!(Sound_Shoot);
sound_preset!(Sound_Powerup);
sound_preset!(Sound_Hurt);
sound_preset!(Sound_Click);
sound_preset!(Sound_Bang);
sound_preset!(Sound_Blip);
sound_preset!(Sound_Pickup);

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_SweepUp(
    slot: i64,
    _start_freq: f64,
    _end_freq: f64,
    duration: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_SweepDown(
    slot: i64,
    _start_freq: f64,
    _end_freq: f64,
    duration: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_RandomBeep(slot: i64, _seed: i64, duration: f64) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Tone(slot: i64, _freq: f64, duration: f64, _waveform: i64) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Note(
    slot: i64,
    _midi: i64,
    duration: f64,
    _waveform: i64,
    _attack: f64,
    _decay: f64,
    _sustain: f64,
    _release: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Noise(slot: i64, _noise_type: i64, duration: f64) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_FM(
    slot: i64,
    _carrier_hz: f64,
    _modulator_hz: f64,
    _mod_index: f64,
    duration: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Reverb(
    slot: i64,
    _frequency: f64,
    duration: f64,
    _waveform: i64,
    _room_size: f64,
    _damping: f64,
    _wet: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Delay(
    slot: i64,
    _frequency: f64,
    duration: f64,
    _waveform: i64,
    _delay_time: f64,
    _feedback: f64,
    _mix: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Distort(
    slot: i64,
    _frequency: f64,
    duration: f64,
    _waveform: i64,
    _drive: f64,
    _tone: f64,
    _level: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_FilterTone(
    slot: i64,
    _frequency: f64,
    duration: f64,
    _waveform: i64,
    _filter_type: i64,
    _cutoff: f64,
    _resonance: f64,
) -> i64 {
    store_sound(slot, duration)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_FilterNote(
    slot: i64,
    _midi: i64,
    duration: f64,
    _waveform: i64,
    _attack: f64,
    _decay: f64,
    _sustain: f64,
    _release: f64,
    _filter_type: i64,
    _cutoff: f64,
    _resonance: f64,
) -> i64 {
    store_sound(slot, duration)
}

// ─── playback / management ──────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Play(slot: i64, _volume: f64, _pan: f64) -> i64 {
    with_state(|s| {
        if s.sounds.contains_key(&slot) {
            BCPL_AUDIO_OK
        } else {
            BCPL_AUDIO_ERR_UNKNOWN_SLOT
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_StopAll() -> i64 {
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Free(slot: i64) -> i64 {
    with_state(|s| s.sounds.remove(&slot));
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_FreeAll() -> i64 {
    with_state(|s| s.sounds.clear());
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_SetVolume(volume: f64) -> i64 {
    with_state(|s| s.sound_volume = volume.clamp(0.0, 1.0));
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_GetVolume() -> f64 {
    with_state(|s| s.sound_volume)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Count() -> i64 {
    with_state(|s| s.sounds.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Playing(_slot: i64) -> i64 {
    // No live playback in the stub — nothing is ever "currently playing".
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Sound_Duration(slot: i64) -> f64 {
    with_state(|s| s.sounds.get(&slot).copied().unwrap_or(0.0))
}

// ─── Music (ABC) — slot bookkeeping only ────────────────────────────

/// Load an ABC tune string into `slot`. The stub does not parse the
/// tune; it just marks the slot as loaded with a default tempo.
///
/// # Safety
/// `abc_ptr` must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn Music_Load(slot: i64, abc_ptr: *const u8) -> i64 {
    if abc_ptr.is_null() {
        return BCPL_AUDIO_ERR_PARSE;
    }
    with_state(|s| {
        s.music.insert(slot, 120.0);
    });
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_Play(slot: i64, _volume: f64) -> i64 {
    with_state(|s| {
        s.music.entry(slot).or_insert(120.0);
        s.music_state = MUSIC_STATE_PLAYING;
    });
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_StopAll() -> i64 {
    with_state(|s| s.music_state = MUSIC_STATE_STOPPED);
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_PauseAll() -> i64 {
    with_state(|s| {
        if s.music_state == MUSIC_STATE_PLAYING {
            s.music_state = MUSIC_STATE_PAUSED;
        }
    });
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_ResumeAll() -> i64 {
    with_state(|s| {
        if s.music_state == MUSIC_STATE_PAUSED {
            s.music_state = MUSIC_STATE_PLAYING;
        }
    });
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_Free(slot: i64) -> i64 {
    with_state(|s| s.music.remove(&slot));
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_FreeAll() -> i64 {
    with_state(|s| {
        s.music.clear();
        s.music_state = MUSIC_STATE_STOPPED;
    });
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_SetVolume(volume: f64) -> i64 {
    with_state(|s| s.music_volume = volume.clamp(0.0, 1.0));
    BCPL_AUDIO_OK
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_GetVolume() -> f64 {
    with_state(|s| s.music_volume)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_Count() -> i64 {
    with_state(|s| s.music.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_State() -> i64 {
    with_state(|s| s.music_state)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_Playing(_slot: i64) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn Music_Tempo(slot: i64) -> f64 {
    with_state(|s| s.music.get(&slot).copied().unwrap_or(0.0))
}
