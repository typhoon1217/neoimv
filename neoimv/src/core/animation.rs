use std::time::{Duration, Instant};

pub struct AnimationFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay: Duration,
}

pub struct AnimationState {
    frames: Vec<AnimationFrame>,
    current_frame: usize,
    last_frame_time: Instant,
    playing: bool,
}

impl AnimationState {
    pub fn new(frames: Vec<AnimationFrame>) -> Self {
        Self {
            frames,
            current_frame: 0,
            last_frame_time: Instant::now(),
            playing: true,
        }
    }

    /// Returns true if there is more than one frame (i.e. this is animated).
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Toggle play/pause. Resets the frame timer when resuming.
    pub fn toggle_playback(&mut self) {
        self.playing = !self.playing;
        if self.playing {
            self.last_frame_time = Instant::now();
        }
    }

    pub fn play(&mut self) {
        if !self.playing {
            self.playing = true;
            self.last_frame_time = Instant::now();
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.current_frame = 0;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn current_frame_index(&self) -> usize {
        self.current_frame
    }

    /// Advance the frame if enough time has elapsed for the current frame's delay.
    /// Returns true if the frame changed (caller should update the texture).
    pub fn tick(&mut self) -> bool {
        if !self.playing || self.frames.len() <= 1 {
            return false;
        }
        if self.last_frame_time.elapsed() >= self.frames[self.current_frame].delay {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            self.last_frame_time = Instant::now();
            return true;
        }
        false
    }

    /// Returns the pixel data for the current frame.
    pub fn current_frame(&self) -> Option<&AnimationFrame> {
        self.frames.get(self.current_frame)
    }

    /// Returns the delay of the current frame, or a default 100ms if no frames.
    pub fn current_delay(&self) -> Duration {
        self.frames
            .get(self.current_frame)
            .map(|f| f.delay)
            .unwrap_or(Duration::from_millis(100))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(delay_ms: u64) -> AnimationFrame {
        AnimationFrame {
            pixels: vec![0u8; 16],
            width: 2,
            height: 2,
            delay: Duration::from_millis(delay_ms),
        }
    }

    #[test]
    fn test_single_frame_not_animated() {
        let state = AnimationState::new(vec![make_frame(100)]);
        assert!(!state.is_animated());
    }

    #[test]
    fn test_two_frames_is_animated() {
        let state = AnimationState::new(vec![make_frame(100), make_frame(100)]);
        assert!(state.is_animated());
    }

    #[test]
    fn test_tick_does_not_advance_single_frame() {
        let mut state = AnimationState::new(vec![make_frame(1)]);
        std::thread::sleep(Duration::from_millis(5));
        assert!(!state.tick());
        assert_eq!(state.current_frame_index(), 0);
    }

    #[test]
    fn test_tick_advances_frame() {
        let mut state = AnimationState::new(vec![make_frame(1), make_frame(1)]);
        std::thread::sleep(Duration::from_millis(5));
        assert!(state.tick());
        assert_eq!(state.current_frame_index(), 1);
    }

    #[test]
    fn test_tick_wraps_around() {
        let mut state = AnimationState::new(vec![make_frame(1), make_frame(1)]);
        // Advance to frame 1
        std::thread::sleep(Duration::from_millis(5));
        state.tick();
        assert_eq!(state.current_frame_index(), 1);
        // Advance back to frame 0
        std::thread::sleep(Duration::from_millis(5));
        state.tick();
        assert_eq!(state.current_frame_index(), 0);
    }

    #[test]
    fn test_pause_stops_tick() {
        let mut state = AnimationState::new(vec![make_frame(1), make_frame(1)]);
        state.pause();
        std::thread::sleep(Duration::from_millis(5));
        assert!(!state.tick());
        assert_eq!(state.current_frame_index(), 0);
    }

    #[test]
    fn test_stop_resets_to_first_frame() {
        let mut state = AnimationState::new(vec![make_frame(1), make_frame(1)]);
        std::thread::sleep(Duration::from_millis(5));
        state.tick();
        assert_eq!(state.current_frame_index(), 1);
        state.stop();
        assert_eq!(state.current_frame_index(), 0);
        assert!(!state.is_playing());
    }

    #[test]
    fn test_toggle_playback() {
        let mut state = AnimationState::new(vec![make_frame(100), make_frame(100)]);
        assert!(state.is_playing());
        state.toggle_playback();
        assert!(!state.is_playing());
        state.toggle_playback();
        assert!(state.is_playing());
    }
}
