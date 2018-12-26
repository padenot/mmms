extern crate audio_clock;
extern crate bela;
extern crate euclidian_rythms;
extern crate mbms_traits;
extern crate monome;
extern crate smallvec;

use std::cmp;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::{thread, time};
use std::fmt;
use std::str::FromStr;

use audio_clock::*;
use bela::*;
use euclidian_rythms::*;
use mbms_traits::*;
use monome::{KeyDirection, MonomeEvent};
use smallvec::SmallVec;

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
            MonomeEvent::GridKey { x, y, direction } => {
                match direction {
                    KeyDirection::Down => {
                        self.state_tracker.down(x as usize, y as usize);
                    }
                    KeyDirection::Up => {
                        match self.state_tracker.up(x as usize, y as usize) {
                            MMMSAction::Tick((x, y)) => {
                                println!("tick");
                            }
                            _ => {
                                println!("nothing");
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
enum PitchClass {
    A, B, C, D, E, F, G
}

impl PitchClass {
    // todo: replace with real try_from when it's stable
    fn try_from(c: char) -> Result<Self, ()> {
        match c {
            'A'|'a' => Ok(PitchClass::A),
            'B'|'b' => Ok(PitchClass::B),
            'C'|'c' => Ok(PitchClass::C),
            'D'|'d' => Ok(PitchClass::D),
            'E'|'e' => Ok(PitchClass::E),
            'F'|'f' => Ok(PitchClass::F),
            'G'|'g' => Ok(PitchClass::G),
            _ => Err(())
        }
    }
}


impl fmt::Display for PitchClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
enum Accidental {
    Flat,
    Natural,
    Sharp
}

impl Accidental {
    // todo: replace with real try_from when it's stable
    fn try_from(c: char) -> Result<Self, ()> {
        match c {
            'b'|'♭' => Ok(Accidental::Flat),
            '♮' => Ok(Accidental::Natural),
            '#'|'♯' => Ok(Accidental::Sharp),
            _ => Err(())
        }
    }
}

impl fmt::Display for Accidental {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let symbol = match self {
            Accidental::Flat => "♭",
            Accidental::Natural => "",
            Accidental::Sharp => "♯",
        };
        write!(f, "{}", symbol)
    }
}

#[derive(Debug, Clone)]
struct Pitch {
    pitch_class: PitchClass,
    accidental: Accidental,
    octave: i8,
}

impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}{}", self.pitch_class, self.accidental, self.octave)
    }
}

impl Pitch {
    fn try_from(string: &str) -> Result<Pitch,()> {
        Self::parse(string)
    }
    fn new(pitch_class: PitchClass, accidental: Accidental, octave: i8) -> Pitch {
        Pitch {
            pitch_class,
            accidental,
            octave
        }
    }
    /// Parse a string representation into a pitch. Weird notation are accepted, such as "B#4" or
    /// "A♮4"
    fn parse(string: &str) -> Result<Pitch,()> {
      if string.chars().count() < 2 || string.chars().count() > 3 {
          return Err(());
      }

      let mut it = string.char_indices().peekable();

      let pitch_class = PitchClass::try_from(it.next().unwrap().1)?;
      // accidental is not mandatory, if it's not present it's natural
      let maybe_accidental = it.peek().unwrap().1;
      let accidental = match Accidental::try_from(maybe_accidental) {
          Ok(a) => {
              it.next();
              a
          }
          _ => {
              Accidental::Natural
          }
      };
      let (idx, char) = it.next().unwrap();
      let (begin, octave_string) = string.split_at(idx);
      let maybe_octave = octave_string.parse::<i8>();
      let octave = match maybe_octave {
          Ok(o) => {
              o
          }
          _ => {
              return Err(());
          }
      };

      Ok(Pitch {
          pitch_class,
          accidental,
          octave
      })
    }
    /// Returns a number of Volts for this note, to control the pitch via a control voltage (CV).
    /// This is fairly arbitrary, apart from the fact that one volt is one octave. This system
    /// considers that C0 is 0V.
    fn to_CV(&self) -> f32 {
      (self.octave as f32) + ((self.semitone_offset() + self.accidental_offset()) as f32 / 12.)
    }
    /// Returns the pitch of this note in Hertz
    fn to_Hz(&self) -> f32 {
        440. * (2. as f32).powf(((self.to_MIDI() as f32) - 69.) / 12.)
    }
    /// Number of semitones from the base C for this note
    fn semitone_offset(&self) -> i8 {
        match self.pitch_class {
            PitchClass::A => { 9 }
            PitchClass::B => { 11 }
            PitchClass::C => { 0 }
            PitchClass::D => { 2 }
            PitchClass::E => { 4 }
            PitchClass::F => { 5 }
            PitchClass::G => { 7 }
        }
    }
    /// -1, 0 or 1, depending, for this note, based on which accidental it has
    fn accidental_offset(&self) -> i8 {
        match self.accidental {
            Accidental::Flat => { -1 }
            Accidental::Natural => { 0 }
            Accidental::Sharp => { 1 }
        }
    }
    /// Returns a MIDI note number, from the Scientific Pitch Notation
    /// <https://en.wikipedia.org/wiki/Scientific_pitch_notation>
    fn to_MIDI(&self) -> i8 {
      let base_octave = (self.octave + 1) * 12;
      let offset = self.semitone_offset();
      let accidental = self.accidental_offset();
      return base_octave + offset + accidental;
    }
}


#[derive(Debug, Clone)]
struct Note {
  pitch: Pitch,
  // steps for now, handle triplets and such later...
  duration: u8
}

struct Sequence {
    steps: SmallVec::<[Option<Note>; 64]>
}

impl Sequence {
    fn new() -> Sequence
    {
       let mut steps = SmallVec::<[Option<Note>; 64]>::new();
       steps.resize(16, None);
       Sequence {
           steps,
       }
    }
    fn resize(&mut self, new_size: usize) {
        self.steps.resize(new_size, None);
    }
    fn press(&mut self, x: usize, y: Note) {
    }
}

#[cfg(test)]
mod tests {
    use Pitch;

    #[test]
    fn it_works() {
        let notes = ["a4", "A4", "C-1", "Cb1", "F#3", "B♮1"];
        let midi = [69, 69, 0, 23, 54, 35];
        let Hz = [440., 440., 8.1758, 30.868, 185.00, 61.735];
        for i in 0..notes.len() {
            let note = Pitch::try_from(notes[i]).unwrap();
            println!("{} {} {} {}",note, note.to_MIDI(), note.to_CV(), note.to_Hz());
            assert!(note.to_MIDI() == midi[i]);
            assert!((note.to_Hz() - Hz[i]).abs() < 0.01);
        }
        let bad_notes = ["i4", "4a", "C&1", "asdasdasd", "#A4", "♮♮♮"];
        for i in 0..notes.len() {
            assert!(Pitch::try_from(bad_notes[i]).is_err());
        }
    }
}
