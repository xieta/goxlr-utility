#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("Expected int: {0}")]
    ExpectedInt(#[from] std::num::ParseIntError),

    #[error("Expected float: {0}")]
    ExpectedFloat(#[from] std::num::ParseFloatError),

    #[error("Expected enum: {0}")]
    ExpectedEnum(#[from] strum::ParseError),

    #[error("Invalid browser: {0}")]
    InvalidBrowser(#[from] crate::components::browser::ParseError),

    #[error("Invalid colours: {0}")]
    InvalidColours(#[from] crate::components::colours::ParseError),

    #[error("Invalid context: {0}")]
    InvalidContext(#[from] crate::components::context::ParseError),

    #[error("Invalid echo: {0}")]
    InvalidEcho(#[from] crate::components::echo::ParseError),

    #[error("Invalid effects: {0}")]
    InvalidEffects(#[from] crate::components::effects::ParseError),

    #[error("Invalid fader: {0}")]
    InvalidFader(#[from] crate::components::fader::ParseError),

    #[error("Invalid gender: {0}")]
    InvalidGender(#[from] crate::components::gender::ParseError),

    #[error("Invalid hardtune: {0}")]
    InvalidHardtune(#[from] crate::components::hardtune::ParseError),

    #[error("Invalid megaphone: {0}")]
    InvalidMegaphone(#[from] crate::components::megaphone::ParseError),

    #[error("Invalid mixer: {0}")]
    InvalidMixer(#[from] crate::components::mixer::ParseError),

    #[error("Invalid mute: {0}")]
    InvalidMute(#[from] crate::components::mute::ParseError),

    #[error("Invalid mute_chat: {0}")]
    InvalidMuteChat(#[from] crate::components::mute_chat::ParseError),

    #[error("Invalid pitch: {0}")]
    InvalidPitch(#[from] crate::components::pitch::ParseError),

    #[error("Invalid reverb: {0}")]
    InvalidReverb(#[from] crate::components::reverb::ParseError),

    #[error("Invalid robot: {0}")]
    InvalidRobot(#[from] crate::components::robot::ParseError),

    #[error("Invalid root: {0}")]
    InvalidRoot(#[from] crate::components::root::ParseError),

    #[error("Invalid sample: {0}")]
    InvalidSample(#[from] crate::components::sample::ParseError),

    #[error("Invalid scribble: {0}")]
    InvalidScribble(#[from] crate::components::scribble::ParseError),

    #[error("Invalid simple: {0}")]
    InvalidSimple(#[from] crate::components::simple::ParseError),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
}