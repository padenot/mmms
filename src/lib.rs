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
const MAX_STEPS: usize = 128;
/// Initial number of steps in the sequencer, in sixteenth.
const INITIAL_STEPS: usize = 32;
/// Number of notes that can be represented, in semitones.
const MAX_NOTES: usize = 128;

pub fn clamp<T: PartialOrd>(input: T, min: T, max: T) -> T {
    debug_assert!(min <= max, "min must be less than or equal to max");
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}


#[derive(Debug)]
enum Message {
    Tick((usize, usize)),
    Scale(Scale),
    Resize(usize),
    Start,
    Stop,
    TempoChange(f32),
}

pub struct MMMSRenderer {
    clock_updater: ClockUpdater,
    clock_consumer: ClockConsumer,
    receiver: Receiver<Message>,
    tempo: f32,
    steps: SmallVec<[Option<Pitch>; 64]>,
    scale: Scale,
    trigger_port: BelaPort,
    pitch_port: BelaPort,
    prev_pitch: f32
}

impl MMMSRenderer {
    fn new(
        width: usize,
        height: usize,
        clock_updater: ClockUpdater,
        clock_consumer: ClockConsumer,
        receiver: Receiver<Message>,
        trigger_port: BelaPort,
        pitch_port: BelaPort
    ) -> MMMSRenderer {
        let mut steps = SmallVec::<[Option<Pitch>; 64]>::new();
        steps.resize(INITIAL_STEPS, None);
        let scale = Scale::new(PitchClass::B, Accidental::Natural, ScaleType::MinorPentatonic);
        MMMSRenderer {
            receiver,
            clock_updater,
            clock_consumer,
            tempo: 0.,
            trigger_port,
            pitch_port,
            steps,
            scale,
            prev_pitch: 0.0
        }
    }
    fn press(&mut self, x: usize, y: usize) {
        self.steps[x] = Some(self.scale.idx_to_pitch(self.scale.note_count() - 1 - y).unwrap())
    }
    fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
    }
    fn set_scale(&mut self, scale: Scale) {
        for i in self.steps.iter_mut() {
            *i = None;
        }
        self.scale = scale;
    }
    fn resize(&mut self, new_size: usize) {
        self.steps.resize(new_size, None);
    }
    fn print_seq(&self) {
        for step in self.steps.iter() {
            if step.is_some() {
                print!("{}\t", step.clone().unwrap());
            } else {
                print!("  \t");
            }
        }
        println!("");
    }
}

impl InstrumentRenderer for MMMSRenderer {
    fn render(&mut self, context: &mut Context) {
        match self.receiver.try_recv() {
            Ok(msg) => match msg {
                Message::Tick((x, y)) => {
                    self.press(x, y);
                }
                Message::Start => {}
                Message::Stop => {}
                Message::Resize(new_size) => {
                    self.resize(new_size)
                }
                Message::TempoChange(tempo) => {
                    self.set_tempo(tempo);
                }
                Message::Scale(scale) => {
                    self.set_scale(scale);
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
        let analog_period = 1. / context.analog_sample_rate();
        let digital_period = 1. / context.digital_sample_rate();
        let beat = self.clock_consumer.beat();
        let sixteenth = beat * 4.;
        let trigger_duration = 0.01; // 10ms

        match self.trigger_port {
            BelaPort::AnalogOut(n) => {
                let mut sixteenth = beat * 4.;
                let analog_channels = context.analog_out_channels();
                let analog_frames = context.analog_frames();
                let analog_out = context.analog_out();
                for i in 0..analog_frames {
                    let integer_sixteenth = sixteenth as usize % self.steps.len();
                    let pitch = &self.steps[integer_sixteenth];
                    if pitch.is_some() && sixteenth.fract() < trigger_duration {
                        println!("playing {}", pitch.clone().unwrap());
                        analog_out[i * analog_channels + n] = 1.0;
                    } else {
                        analog_out[i * analog_channels + n] = 0.0;
                    }
                    sixteenth += analog_period;
                }
            }
            BelaPort::Digital(n) => {
                let digital_frames = context.digital_frames();
                let mut sixteenth = beat * 4.;
                for frame in 0..digital_frames {
                    let integer_sixteenth = sixteenth as usize % self.steps.len();
                    let pitch = &self.steps[integer_sixteenth];
                    if pitch.is_some() && sixteenth.fract() < trigger_duration {
                        println!("playing {}", pitch.clone().unwrap());
                        context.digital_write_once(frame, n, 1);
                    } else {
                        context.digital_write_once(frame, n, 0);
                    }
                    sixteenth += digital_period;
                }
            }
            _ => {
                panic!("wrong ports.");
            }
        }
        if let BelaPort::AnalogOut(channel) = self.pitch_port {
            let analog_channels = context.analog_out_channels();
            let analog_frames = context.analog_frames();
            let analog_out = context.analog_out();
            let mut sixteenth = beat * 4.;
            for i in 0..analog_frames {
                let integer_sixteenth = sixteenth as usize % self.steps.len();
                let pitch = &self.steps[integer_sixteenth];

                // divide by ten to map to the bela range:
                // 0 -> 1.0 is 0 -> 5v in bela, with then an analog gain of two
                if pitch.is_some() {
                    let value = pitch.clone().unwrap().to_cv() / 10.0;
                    assert!(value <= 1.0);
                    self.prev_pitch = value;
                    analog_out[i * analog_channels + channel] = value;
                } else {
                    analog_out[i * analog_channels + channel] = self.prev_pitch
                }
                sixteenth += analog_period;
            }
        } else {
            panic!("wtf.");
        }

        self.clock_updater.increment(frames);
    }
}

pub struct MMMS {
    tempo: f32,
    width: usize,
    height: usize,
    sender: Sender<Message>,
    audio_clock: ClockConsumer,
    state_tracker: GridStateTracker,
    virtual_grid: VirtualGrid,
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

        let (trigger_port, pitch_port) = ports;

        match pitch_port {
            BelaPort::AnalogOut(_) => {
            }
            _ => {
                panic!("Cannot render CV on GPIO.");
            }
        }

        let virtual_grid = VirtualGrid::new();

        let renderer = MMMSRenderer::new(
            16,
            8,
            clock_updater,
            clock_consumer.clone(),
            receiver,
            trigger_port,
            pitch_port);
        let state_tracker = GridStateTracker::new(16, 8);

        let grid = vec![0 as u8; 128];
        (
            MMMS {
                tempo: 120.,
                width,
                height,
                sender,
                audio_clock: clock_consumer,
                state_tracker,
                virtual_grid,
            },
            renderer,
        )
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
    Move((isize, isize)),
    Resize(usize), // number is the number of bars
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

    fn shift_down(&self) -> bool {
      self.buttons[Self::idx(self.width, 15, 0)] != MMMSIntent::Nothing
    }

    fn down(&mut self, x: usize, y: usize) {
        if y == 0 {
            // control row, rightmost part, does nothing for now, the last one is shift
            if x == 15 {
                self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Tick;
            } else {
                self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Nothing;
            }
        } else {
            self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Tick;
        }
    }
    fn up(&mut self, x: usize, y: usize) -> MMMSAction {
        self.buttons[Self::idx(self.width, x, y)] = MMMSIntent::Nothing;
        if y == 0 {
            if !self.shift_down() {
                match x {
                    8 => {
                        return MMMSAction::Move((-16, 0))
                    }
                    9 => {
                        return MMMSAction::Move((16, 0))
                    }
                    10 => {
                        return MMMSAction::Move((0, -1))
                    }
                    11 => {
                        return MMMSAction::Move((0, 1))
                    }
                    _ => {
                        return MMMSAction::Nothing
                    }
                }
            } else {
                match x {
                    8 => {
                        return MMMSAction::Resize(1)
                    }
                    9 => {
                        return MMMSAction::Resize(2)
                    }
                    10 => {
                        return MMMSAction::Resize(4)
                    }
                    11 => {
                        return MMMSAction::Resize(8)
                    }
                    _ => {
                        return MMMSAction::Nothing
                    }
                }
            }
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
        let pos_in_pattern = (sixteenth as usize) % self.virtual_grid.steps_count();

        grid.iter_mut().map(|x| *x = 0).count();

        self.virtual_grid.viewport(&mut grid[16..]);
        self.virtual_grid.draw();

        // draw octave indicator if shift is not pressed. Otherwise, draw the amount of bars
        if !self.state_tracker.shift_down() {
            let current_octave = self.virtual_grid.current_octave();
            grid[8 + current_octave] = 15;
        } else {
            let bars = self.virtual_grid.steps_count() / 16;
            for i in 0..bars {
                grid[8 + i] = 15;
            }
        }

        // draw playhead if visible
        if self.virtual_grid.x_in_view(pos_in_pattern) {
            for i in 1..self.height + 1 {
                let idx = i * 16 + pos_in_pattern % 16;
                if grid[idx] < 4 {
                    grid[idx] = 4;
                }
            }
        }
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
                        self.virtual_grid.tick(x, y);
                        let xy = self.virtual_grid.vaddress(x, y);
                        self.sender.send(Message::Tick(xy));
                    }
                    MMMSAction::Move((x, y)) => {
                        self.virtual_grid.mouve(x, y);
                    }
                    MMMSAction::Resize(bars) => {
                        self.virtual_grid.change_steps_count(bars * 16);
                        self.sender.send(Message::Resize(bars * 16));
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

/// Handle a grid much larger than a monome 128, and allow inputing and displaying on a monome 128,
/// and scrolling through bars (left/right) and notes (up/down). It is aware of the scale it's
/// representing.
/// 0x0 is top left, 64x128 is bottom right
/// the offset_x and offset_y are the position of the top left corner of the viewport
struct VirtualGrid {
    width: usize,
    height: usize,
    offset_x: usize,
    offset_y: usize,
    scale: Scale,
    grid: SmallVec<[Option<u8>; MAX_STEPS]>,
}

impl VirtualGrid {
    fn new() -> VirtualGrid {
         // This is a lie: the grid is in fact just a vector with the position of the notes that
         // are ticked (or none if it's not been ticked).
         let mut grid = SmallVec::<[Option<u8>; MAX_STEPS]>::new();
         // TODO: pick a scale when starting? random?
         let scale = Scale::new(PitchClass::B, Accidental::Natural, ScaleType::MinorPentatonic);
         // third octave
         let start_offset = scale.note_count() - scale.octave_note_count() * 3 - 7;
         grid.resize(INITIAL_STEPS, None);
         VirtualGrid {
             width: INITIAL_STEPS,
             height: scale.note_count(),
             offset_x: 0,
             offset_y: start_offset,
             scale,
             grid,
         }
    }
    fn steps_count(&self) -> usize {
        self.width
    }
    fn change_steps_count(&mut self, count: usize) {
      assert!(count % 16 == 0);
      self.width = count;
      self.offset_x = clamp((self.offset_x as isize) as isize, 0 as isize, (self.width - 16) as isize) as usize;
      self.grid.resize(count, None);
    }
    fn mouve(&mut self, x: isize, y: isize) {
        self.offset_x = clamp((self.offset_x as isize + x as isize) as isize, 0 as isize, (self.width - 16) as isize) as usize;
        self.offset_y = clamp((self.offset_y as isize + y as isize) as isize, 0 as isize, (self.height - 7) as isize) as usize;
    }
    fn vaddress(&self, vx: usize, vy: usize) -> (usize, usize) {
        let x = vx + self.offset_x;
        let y = vy + self.offset_y;

        assert!(x < self.width);
        assert!(y < self.height);

        (x, y)
    }
    // return a number between 0 and 8 that represents the octave currently in the view
    fn current_octave(&self) -> usize {
        clamp((self.scale.note_count() - (self.offset_y + 7)) / self.scale.octave_note_count(), 0, 8)
    }
    fn in_view(&self, x: usize, y: usize) -> bool {
        y >= self.offset_y && y < self.offset_y + 7 &&
        x >= self.offset_x && x < self.offset_x + 16
    }
    fn x_in_view(&self, x: usize) -> bool {
        x >= self.offset_x && x < self.offset_x + 16
    }
    fn viewport(&self, grid: &mut [u8]) {
        assert!(grid.len() == 7 * 16);
        for i in 0..7 {
            for j in 0..16 {
                let local_idx = i * 16 + j;
                // flip verticaly so that lower notes are at the bottom
                grid[local_idx] = match self.scale.idx_to_degree(self.scale.note_count() - 1 - (self.offset_y + i)) {
                    Ok(Degrees::Tonic) => { 10 }
                    Ok(Degrees::Dominant) => { 6 }
                    Ok(Degrees::Leading) => { 4 }
                    _ => { 0 }
                };
                if self.grid[self.offset_x + j].is_some() &&
                   self.grid[self.offset_x + j].unwrap() == (self.offset_y + i) as u8 {
                    grid[local_idx] = 15;
                }
            }
        }
    }
    fn tick(&mut self, vx: usize, vy: usize) {
        let (x, y) = self.vaddress(vx, vy);
        if self.grid[x].is_some() {
            if self.grid[x].unwrap() == y as u8 {
                self.grid[x] = None;
            } else {
                self.grid[x] = Some(y as u8);
            }
        } else {
            self.grid[x] = Some(y as u8);
        }
    }
    // Draw the grid. The notes in the view are circled. 1 is a ticked note.
    fn draw(&self) {
        println!("######### begin #######");
        for i in 0..self.scale.note_count() {
            for j in 0..self.width + 1 {
                if j == 0 {
                    print!("{}\t", self.scale.idx_to_pitch(self.scale.note_count() - 1 - i).unwrap());
                    continue;
                }
                if self.in_view(j, i) {
                   if self.grid[j - 1].is_some() {
                     print!("|{}|", if self.grid[j - 1].unwrap() == i as u8 { 1 } else { 0 });
                   } else {
                     print!("|0|");
                   }
                } else  {
                   if self.grid[j - 1].is_some() {
                     print!(" {} ", if self.grid[j - 1].unwrap() == i as u8 { 1 } else { 0 });
                   } else {
                     print!(" 0 ");
                   }
                }
            }
            print!("\n");
        }
        println!("#########  end  #######");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() { }
}
