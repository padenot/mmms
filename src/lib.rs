extern crate audio_clock;
extern crate bela;
extern crate euclidian_rythms;
extern crate mbms_traits;
extern crate monome;
extern crate smallvec;
extern crate musical_scales;

use std::cmp;
use std::fmt;
use std::str::FromStr;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::{thread, time};

use audio_clock::*;
use musical_scales::*;
use bela::*;
use euclidian_rythms::*;
use mbms_traits::*;
use monome::{KeyDirection, MonomeEvent};
use smallvec::SmallVec;

/// Maximum number of steps in the sequencer, in sixteenth.
const MAX_STEPS: usize = 64;
/// Number of notes that can be represented, in semitones.
const MAX_NOTES: usize = 128;

#[derive(Debug)]
enum Message {
    Key((usize, usize)),
    Start,
    Stop,
    TempoChange(f32),
}

pub struct MMMSRenderer {
    clock_updater: ClockUpdater,
    clock_consumer: ClockConsumer,
    receiver: Receiver<Message>,
    tempo: f32,
    steps: SmallVec<[u8; 64]>,
    port_range: (BelaPort, BelaPort),
}

impl MMMSRenderer {
    fn new(
        width: usize,
        height: usize,
        clock_updater: ClockUpdater,
        clock_consumer: ClockConsumer,
        receiver: Receiver<Message>,
        port_range: (BelaPort, BelaPort),
    ) -> MMMSRenderer {
        let mut steps = SmallVec::<[u8; 64]>::new();
        MMMSRenderer {
            receiver,
            clock_updater,
            clock_consumer,
            tempo: 0.,
            port_range,
            steps,
        }
    }
    fn press(&mut self, x: usize, pitch: usize) {
        // ...
    }
    fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
    }
}

impl InstrumentRenderer for MMMSRenderer {
    fn render(&mut self, context: &mut Context) {
        match self.receiver.try_recv() {
            Ok(msg) => match msg {
                Message::Key((x, pitch)) => {
                    self.press(x, pitch);
                }
                Message::Start => {}
                Message::Stop => {}
                Message::TempoChange(tempo) => {
                    self.set_tempo(tempo);
                }
            },
            Err(err) => match err {
                std::sync::mpsc::TryRecvError::Empty => {}
                std::sync::mpsc::TryRecvError::Disconnected => {
                    println!("disconnected");
                }
            },
        }

        let frames = context.audio_frames();
        let beat = self.clock_consumer.beat();
        let sixteenth = beat * 4.;
        let trigger_duration = 0.01; // 10ms
        let integer_sixteenth = sixteenth as usize;

        self.clock_updater.increment(frames);
    }
}

pub struct MMMS {
    tempo: f32,
    width: usize,
    height: usize,
    sender: Sender<Message>,
    audio_clock: ClockConsumer,
    grid: Vec<u8>,
    state_tracker: GridStateTracker,
}

impl MMMS {
    pub fn new(
        ports: (BelaPort, BelaPort),
        width: usize,
        height: usize,
        tempo: f32,
    ) -> (MMMS, MMMSRenderer) {
        let (sender, receiver) = channel::<Message>();

        let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

        let portrange = match ports {
            (BelaPort::Digital(start), BelaPort::Digital(end)) => {
                if end - start != height {
                    panic!("not enought output ports");
                }
            }
            (BelaPort::AnalogOut(start), BelaPort::AnalogOut(end)) => {
                if end - start != height {
                    panic!("not enought output ports");
                }
            }
            _ => {
                panic!("bad BelaPort for MMMS");
            }
        };

        let renderer = MMMSRenderer::new(
            16,
            8,
            clock_updater,
            clock_consumer.clone(),
            receiver,
            ports,
        );
        let state_tracker = GridStateTracker::new(16, 8);

        let grid = vec![0 as u8; 128];
        (
            MMMS {
                tempo: 120.,
                width,
                height,
                sender,
                audio_clock: clock_consumer,
                grid,
                state_tracker,
            },
            renderer,
        )
    }

    pub fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
        self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, pitch: usize) {
        self.sender.send(Message::Key((x, pitch)));
    }
}

#[derive(Clone, PartialEq)]
enum MMMSIntent {
    Nothing,
    Tick,
}

#[derive(Debug, Copy, Clone)]
enum MMMSAction {
    Nothing,
    Tick((usize, usize)),
}

struct GridStateTracker {
    buttons: Vec<MMMSIntent>,
    width: usize,
    height: usize,
}

impl GridStateTracker {
    fn new(width: usize, height: usize) -> GridStateTracker {
        GridStateTracker {
            width,
            height,
            buttons: vec![MMMSIntent::Nothing; width * height],
        }
    }

    fn down(&mut self, x: usize, y: usize) {
        if y == 0 {
            // control row, rightmost part, does nothing for now.
            self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Nothing;
        } else {
            self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Tick;
        }
    }
    fn up(&mut self, x: usize, y: usize) -> MMMSAction {
        if y == 0 {
            // control row, nothing for now
            MMMSAction::Nothing
        } else {
            match self.buttons[Self::idx(self.width, x, y)].clone() {
                MMMSIntent::Nothing => {
                    // !? pressed a key during startup
                    MMMSAction::Nothing
                }
                MMMSIntent::Tick => {
                    self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Nothing;
                    MMMSAction::Tick((x, y - 1))
                }
            }
        }
    }
    fn idx(width: usize, x: usize, y: usize) -> usize {
        y * width + x
    }
}

impl InstrumentControl for MMMS {
    fn render(&mut self, grid: &mut [u8; 128]) {
        let now = self.audio_clock.beat();
        let sixteenth = now * 4.;
        let mut steps = [0 as u8; 16];
        let pos_in_pattern = (sixteenth as usize) % self.width;

        grid.iter_mut().map(|x| *x = 0).count();

        // draw playhead
        for i in 1..self.height + 1 {
            grid[i * self.width + pos_in_pattern] = 4;
        }

        // draw notes
        // for i in
    }
    fn main_thread_work(&mut self) {
        // noop
    }
    fn input(&mut self, event: MonomeEvent) {
        match event {
            MonomeEvent::GridKey { x, y, direction } => match direction {
                KeyDirection::Down => {
                    self.state_tracker.down(x as usize, y as usize);
                }
                KeyDirection::Up => match self.state_tracker.up(x as usize, y as usize) {
                    MMMSAction::Tick((x, y)) => {
                        println!("tick");
                    }
                    _ => {
                        println!("nothing");
                    }
                },
            },
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct Note {
    pitch: Pitch,
    // steps for now, handle triplets and such later... Maybe always 1?
    duration: u8,
}

struct Sequence {
    steps: SmallVec<[Option<Note>; 64]>,
    begin: usize,
    end: usize,
}

impl Sequence {
    fn new() -> Sequence {
        let mut steps = SmallVec::<[Option<Note>; 64]>::new();
        steps.resize(16, None);
        Sequence { steps, begin: 0, end: 15 }
    }
    fn resize(&mut self, new_size: usize) {
        self.steps.resize(new_size, None);
        if self.end > new_size - 1 {
            self.end = new_size - 1;
        }
        if self.begin > new_size - 1 {
            self.begin = 0;
        }
    }
    fn press(&mut self, x: usize, note: Note) {
      self.steps[x] = Some(note);
    }
    fn sequence(&self) -> &SmallVec<[Option<Note>; 64]> {
      &self.steps
    }
}

/// Handle a grid much larger than a monome 128, and allow inputing and displaying on a monome 128,
/// and scrolling through bars (left/right) and notes (up/down). It is aware of the scale it's
/// representing.
struct VirtualGrid {
    width: usize,
    height: usize,
    offset_x: usize,
    offset_y: usize,
    scale: Scale,
    grid: SmallVec<[u8; MAX_STEPS * MAX_NOTES]>,
}

impl VirtualGrid {
    fn new(width: usize, height: usize) {
         // pick a scale when starting? random?
         let mut grid = SmallVec::<[Option<Note>; MAX_STEPS * MAX_NOTES]>::new();
         // grid.resize(8 * )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() { }
}
