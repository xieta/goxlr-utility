use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::exit;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use enum_map::EnumMap;
use log::{debug, error, warn};
use strum::EnumProperty;
use strum::IntoEnumIterator;
use xml::reader::XmlEvent as XmlReaderEvent;
use xml::{EmitterConfig, EventReader};
use zip::write::FileOptions;

use crate::components::browser::BrowserPreviewTree;
use crate::components::context::Context;
use crate::components::echo::EchoEncoderBase;
use crate::components::effects::Effects;
use crate::components::fader::Fader;
use crate::components::gender::GenderEncoderBase;
use crate::components::hardtune::HardtuneEffectBase;
use crate::components::megaphone::MegaphoneEffectBase;
use crate::components::mixer::Mixers;
use crate::components::mute::MuteButton;
use crate::components::mute_chat::MuteChat;
use crate::components::pitch::PitchEncoderBase;
use crate::components::reverb::ReverbEncoderBase;
use crate::components::robot::RobotEffectBase;
use crate::components::root::RootElement;
use crate::components::sample::SampleBase;
use crate::components::scribble::Scribble;
use crate::components::simple::{SimpleElement, SimpleElements};
use crate::SampleButtons::{BottomLeft, BottomRight, Clear, TopLeft, TopRight};
use crate::{Faders, Preset, SampleButtons};

#[derive(Debug)]
pub struct Profile {
    settings: ProfileSettings,
    scribbles: [Vec<u8>; 4],
}

impl Profile {
    pub fn load<R: Read + std::io::Seek>(read: R) -> Result<Self> {
        debug!("Loading Profile Archive..");

        let mut archive = zip::ZipArchive::new(read)?;

        let mut scribbles: [Vec<u8>; 4] = Default::default();

        debug!("Checking for Scribbles..");
        // Load the scribbles if they exist, store them in memory for later fuckery.
        for (i, scribble) in scribbles.iter_mut().enumerate() {
            let filename = format!("scribble{}.png", i + 1);
            if let Ok(mut file) = archive.by_name(filename.as_str()) {
                debug!("Reading Scribble: {}", filename);

                *scribble = vec![0; file.size() as usize];
                file.read_exact(scribble)?;
            }
        }

        debug!("Attempting to read profile.xml..");
        let settings = ProfileSettings::load(archive.by_name("profile.xml")?)?;
        Ok(Profile {
            settings,
            scribbles,
        })
    }

    // Ok, this is better.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        debug!("Saving File: {}", &path.as_ref().to_string_lossy());

        // Create a new ZipFile at the requested location
        let mut archive = zip::ZipWriter::new(File::create(path.as_ref())?);

        // Store the profile..
        archive.start_file("profile.xml", FileOptions::default())?;
        self.settings.write_to(&mut archive)?;

        // Write the scribbles..
        for (i, scribble) in self.scribbles.iter().enumerate() {
            // Only write if there's actually data stored..
            if !self.scribbles[i].is_empty() {
                let filename = format!("scribble{}.png", i + 1);
                archive.start_file(filename, FileOptions::default())?;
                archive.write_all(scribble)?;
            }
        }
        archive.finish()?;

        Ok(())
    }

    pub fn settings(&self) -> &ProfileSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut ProfileSettings {
        &mut self.settings
    }

    pub fn get_scribble(&self, id: usize) -> &Vec<u8> {
        &self.scribbles[id]
    }
}

#[derive(Debug)]
pub struct ProfileSettings {
    root: RootElement,
    browser: BrowserPreviewTree,
    mixer: Mixers,
    context: Context,
    mute_chat: MuteChat,
    mute_buttons: EnumMap<Faders, Option<MuteButton>>,
    faders: EnumMap<Faders, Option<Fader>>,
    effects: EnumMap<Preset, Option<Effects>>,
    scribbles: EnumMap<Faders, Option<Scribble>>,
    sampler_map: EnumMap<SampleButtons, Option<SampleBase>>,
    simple_elements: EnumMap<SimpleElements, Option<SimpleElement>>,
    megaphone_effect: MegaphoneEffectBase,
    robot_effect: RobotEffectBase,
    hardtune_effect: HardtuneEffectBase,
    reverb_encoder: ReverbEncoderBase,
    echo_encoder: EchoEncoderBase,
    pitch_encoder: PitchEncoderBase,
    gender_encoder: GenderEncoderBase,
}

impl ProfileSettings {
    pub fn load<R: Read>(read: R) -> Result<Self> {
        debug!("Preparing Structure..");
        let parser = EventReader::new(read);

        let mut root = RootElement::new();
        let mut browser = BrowserPreviewTree::new("browserPreviewTree".to_string());

        let mut mixer = Mixers::new();
        let mut context = Context::new("selectedContext".to_string());
        let mut mute_chat = MuteChat::new("muteChat".to_string());

        let mut mute_buttons: EnumMap<Faders, Option<MuteButton>> = EnumMap::default();
        let mut faders: EnumMap<Faders, Option<Fader>> = EnumMap::default();
        let mut scribbles: EnumMap<Faders, Option<Scribble>> = EnumMap::default();

        let mut effects: EnumMap<Preset, Option<Effects>> = EnumMap::default();

        let mut simple_elements: EnumMap<SimpleElements, Option<SimpleElement>> =
            Default::default();

        let mut megaphone_effect = MegaphoneEffectBase::new("megaphoneEffect".to_string());
        let mut robot_effect = RobotEffectBase::new("robotEffect".to_string());
        let mut hardtune_effect = HardtuneEffectBase::new("hardtuneEffect".to_string());
        let mut reverb_encoder = ReverbEncoderBase::new("reverbEncoder".to_string());
        let mut echo_encoder = EchoEncoderBase::new("echoEncoder".to_string());
        let mut pitch_encoder = PitchEncoderBase::new("pitchEncoder".to_string());
        let mut gender_encoder = GenderEncoderBase::new("genderEncoder".to_string());

        let mut sampler_map: EnumMap<SampleButtons, Option<SampleBase>> = EnumMap::default();

        let mut active_sample_button = None;

        debug!("Parsing XML..");
        for e in parser {
            match e {
                Ok(XmlReaderEvent::StartElement {
                    name, attributes, ..
                }) => {
                    debug!("Handling Tag: {}", name.local_name);
                    if name.local_name == "ValueTreeRoot" {
                        // This also handles <AppTree, due to a single shared value.
                        root.parse_root(&attributes)?;

                        // This code was made for XML version 2, v1 not currently supported.
                        if root.get_version() > 2 {
                            println!("XML Version Not Supported: {}", root.get_version());
                            exit(-1);
                        }

                        if root.get_version() < 2 {
                            println!(
                                "XML Version {} detected, will be upgraded to v2",
                                root.get_version()
                            );
                        }
                        continue;
                    }

                    if name.local_name == "browserPreviewTree" {
                        browser.parse_browser(&attributes)?;
                        continue;
                    }

                    if name.local_name == "mixerTree" {
                        mixer.parse_mixers(&attributes)?;
                        continue;
                    }

                    if name.local_name == "selectedContext" {
                        context.parse_context(&attributes)?;
                        continue;
                    }

                    if name.local_name == "muteChat" {
                        mute_chat.parse_mute_chat(&attributes)?;
                        continue;
                    }

                    // Might need to pattern match this..
                    if name.local_name.starts_with("mute") && name.local_name != "muteChat" {
                        // In the XML, the count starts as 1, here, we're gonna store as 0.
                        if let Some(id) = name
                            .local_name
                            .chars()
                            .last()
                            .map(|s| u8::from_str(&s.to_string()))
                            .transpose()?
                        {
                            let mut mute_button = MuteButton::new(id);
                            mute_button.parse_button(&attributes)?;
                            mute_buttons[Faders::iter().nth((id - 1).into()).unwrap()] =
                                Some(mute_button);
                            continue;
                        }
                    }

                    if name.local_name.starts_with("FaderMeter") {
                        // In the XML, the count starts at 0, and we have different capitalisation :D
                        if let Some(id) = name
                            .local_name
                            .chars()
                            .last()
                            .map(|s| u8::from_str(&s.to_string()))
                            .transpose()?
                        {
                            let mut fader = Fader::new(id);
                            fader.parse_fader(&attributes)?;
                            faders[Faders::iter().nth(id.into()).unwrap()] = Some(fader);
                            continue;
                        }
                    }

                    if name.local_name.starts_with("effects") {
                        let mut found = false;

                        // Version 2, now with more enum, search for the prefix..
                        for preset in Preset::iter() {
                            if preset.get_str("contextTitle").unwrap() == name.local_name {
                                let mut effect = Effects::new(preset);
                                effect.parse_effect(&attributes)?;
                                effects[preset] = Some(effect);
                                found = true;
                                break;
                            }
                        }
                        if found {
                            continue;
                        }
                    }

                    if name.local_name.starts_with("scribble") {
                        if let Some(id) = name
                            .local_name
                            .chars()
                            .last()
                            .map(|s| u8::from_str(&s.to_string()))
                            .transpose()?
                        {
                            let mut scribble = Scribble::new(id);
                            scribble.parse_scribble(&attributes)?;
                            scribbles[Faders::iter().nth((id - 1).into()).unwrap()] =
                                Some(scribble);
                            continue;
                        }
                    }

                    if name.local_name == "megaphoneEffect" {
                        megaphone_effect.parse_megaphone_root(&attributes)?;
                        continue;
                    }

                    // Because the depth is crazy small, and tag names don't ever repeat themselves, there's really no point
                    // tracking the opening and closing of tags except when writing, so we'll continue treating the reading
                    // as if it were a very flat structure.
                    if name.local_name.starts_with("megaphoneEffectpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            megaphone_effect.parse_megaphone_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "robotEffect" {
                        robot_effect.parse_robot_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("robotEffectpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            robot_effect.parse_robot_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "hardtuneEffect" {
                        hardtune_effect.parse_hardtune_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("hardtuneEffectpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            hardtune_effect.parse_hardtune_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "reverbEncoder" {
                        reverb_encoder.parse_reverb_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("reverbEncoderpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            reverb_encoder.parse_reverb_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "echoEncoder" {
                        echo_encoder.parse_echo_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("echoEncoderpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            echo_encoder.parse_echo_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "pitchEncoder" {
                        pitch_encoder.parse_pitch_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("pitchEncoderpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            pitch_encoder.parse_pitch_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    if name.local_name == "genderEncoder" {
                        gender_encoder.parse_gender_root(&attributes)?;
                        continue;
                    }

                    if name.local_name.starts_with("genderEncoderpreset") {
                        if let Ok(preset) = ProfileSettings::parse_preset(name.local_name.clone()) {
                            gender_encoder.parse_gender_preset(preset, &attributes)?;
                            continue;
                        }
                    }

                    // These can probably be a little cleaner..
                    if name.local_name == "sampleTopLeft" {
                        let mut sampler = SampleBase::new("sampleTopLeft".to_string());
                        sampler.parse_sample_root(&attributes)?;
                        sampler_map[TopLeft] = Some(sampler);
                        active_sample_button = sampler_map[TopLeft].as_mut();
                        continue;
                    }

                    if name.local_name == "sampleTopRight" {
                        let mut sampler = SampleBase::new("sampleTopRight".to_string());
                        sampler.parse_sample_root(&attributes)?;
                        sampler_map[TopRight] = Some(sampler);
                        active_sample_button = sampler_map[TopRight].as_mut();
                        continue;
                    }

                    if name.local_name == "sampleBottomLeft" {
                        let mut sampler = SampleBase::new("sampleBottomLeft".to_string());
                        sampler.parse_sample_root(&attributes)?;
                        sampler_map[BottomLeft] = Some(sampler);
                        active_sample_button = sampler_map[BottomLeft].as_mut();
                        continue;
                    }

                    if name.local_name == "sampleBottomRight" {
                        let mut sampler = SampleBase::new("sampleBottomRight".to_string());
                        sampler.parse_sample_root(&attributes)?;
                        sampler_map[BottomRight] = Some(sampler);
                        active_sample_button = sampler_map[BottomRight].as_mut();
                        continue;
                    }

                    if name.local_name == "sampleClear" {
                        let mut sampler = SampleBase::new("sampleClear".to_string());
                        sampler.parse_sample_root(&attributes)?;
                        sampler_map[Clear] = Some(sampler);
                        active_sample_button = sampler_map[Clear].as_mut();
                        continue;
                    }

                    if name.local_name.starts_with("sampleStack") {
                        if let Some(id) = name.local_name.chars().last() {
                            if let Some(button) = &mut active_sample_button {
                                button.parse_sample_stack(id, &attributes)?;
                                continue;
                            }
                        }
                    }

                    if name.local_name.starts_with("sampleBank")
                        || name.local_name == "fxClear"
                        || name.local_name == "swear"
                        || name.local_name == "globalColour"
                        || name.local_name == "logoX"
                    {
                        // In this case, the tag name, and attribute prefixes are the same..
                        let mut simple_element = SimpleElement::new(name.local_name.clone());
                        simple_element.parse_simple(&attributes)?;
                        simple_elements[SimpleElements::from_str(&name.local_name)?] =
                            Some(simple_element);

                        continue;
                    }

                    if name.local_name == "AppTree" {
                        // This is handled by ValueTreeRoot
                        continue;
                    }

                    warn!("Unhandled Tag: {}", name.local_name);
                }

                Ok(XmlReaderEvent::EndElement { name }) => {
                    // This probably isn't needed, but cleans up the variable once the stacks have been
                    // read.
                    if name.local_name == "sampleTopLeft"
                        || name.local_name == "sampleTopRight"
                        || name.local_name == "sampleBottomLeft"
                        || name.local_name == "sampleBottomRight"
                        || name.local_name == "sampleClear"
                    {
                        active_sample_button = None;
                    }
                }
                Err(e) => {
                    error!("Error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(Self {
            root,
            browser,
            mixer,
            context,
            mute_chat,
            mute_buttons,
            faders,
            effects,
            scribbles,
            sampler_map,
            simple_elements,
            megaphone_effect,
            robot_effect,
            hardtune_effect,
            reverb_encoder,
            echo_encoder,
            pitch_encoder,
            gender_encoder,
        })
    }

    pub fn load_preset<R: Read>(&mut self, read: R) -> Result<()> {
        // So, in principle here, all we need to do is loop over the tags, check on the
        // tag name, and load it directly into the relevant effect. This should force a
        // replace of the current effect, and bam, done.

        // Firstly, we need the current preset to overwrite.
        let current = self.context().selected_effects();

        // Create the parser..
        let parser = EventReader::new(read);

        let mut read_top = false;
        for e in parser {
            match e {
                Ok(XmlReaderEvent::StartElement {
                    name, attributes, ..
                }) => {
                    if !read_top {
                        // This is the first 'Start Element', which will be the top level element.
                        for attribute in attributes {
                            if attribute.name.local_name == "name" {
                                read_top = true;
                                self.effects_mut(current)
                                    .set_name(attribute.value.clone())?;
                                break;
                            }
                        }

                        if !read_top {
                            return Err(anyhow!("Preset Name not Found, cannot proceed."));
                        }
                        continue;
                    }

                    match name.local_name.as_str() {
                        "reverbEncoder" => self
                            .reverb_encoder
                            .parse_reverb_preset(current, &attributes)?,
                        "echoEncoder" => {
                            self.echo_encoder.parse_echo_preset(current, &attributes)?
                        }
                        "pitchEncoder" => self
                            .pitch_encoder
                            .parse_pitch_preset(current, &attributes)?,
                        "genderEncoder" => self
                            .gender_encoder
                            .parse_gender_preset(current, &attributes)?,
                        "megaphoneEffect" => self
                            .megaphone_effect
                            .parse_megaphone_preset(current, &attributes)?,
                        "robotEffect" => {
                            self.robot_effect.parse_robot_preset(current, &attributes)?
                        }
                        "hardtuneEffect" => self
                            .hardtune_effect
                            .parse_hardtune_preset(current, &attributes)?,
                        _ => warn!("Unexpected Start Tag {}", name.local_name),
                    }
                }
                Err(error) => error!("Error Occurred: {}", error),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let out_file = File::create(path)?;
        self.write_to(out_file)
    }

    pub fn write_to<W: Write>(&self, mut sink: W) -> Result<()> {
        // Create the file, and the writer..
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(true)
            .create_writer(&mut sink);

        // Write the initial root tag..
        self.root.write_initial(&mut writer)?;
        self.browser.write_browser(&mut writer)?;

        self.mixer.write_mixers(&mut writer)?;
        self.context.write_context(&mut writer)?;
        self.mute_chat.write_mute_chat(&mut writer)?;

        for (_fader, mute_button) in self.mute_buttons.iter() {
            if let Some(mute_button) = mute_button {
                mute_button.write_button(&mut writer)?;
            }
        }

        for (_fader, fader) in self.faders.iter() {
            if let Some(fader) = fader {
                fader.write_fader(&mut writer)?;
            }
        }

        for (_key, value) in &self.effects {
            if let Some(value) = value {
                value.write_effects(&mut writer)?;
            }
        }

        for (_fader, scribble) in self.scribbles.iter() {
            if let Some(scribble) = scribble {
                scribble.write_scribble(&mut writer)?;
            }
        }

        self.megaphone_effect.write_megaphone(&mut writer)?;
        self.robot_effect.write_robot(&mut writer)?;
        self.hardtune_effect.write_hardtune(&mut writer)?;

        self.reverb_encoder.write_reverb(&mut writer)?;
        self.echo_encoder.write_echo(&mut writer)?;
        self.pitch_encoder.write_pitch(&mut writer)?;
        self.gender_encoder.write_gender(&mut writer)?;

        for (_key, value) in &self.sampler_map {
            if let Some(value) = value {
                value.write_sample(&mut writer)?;
            }
        }

        for simple_element in SimpleElements::iter() {
            self.simple_elements[simple_element]
                .as_ref()
                .unwrap()
                .write_simple(&mut writer)?;
        }

        // Finalise the XML..
        self.root.write_final(&mut writer)?;

        Ok(())
    }

    pub fn parse_preset(key: String) -> Result<Preset> {
        if let Some(id) = key
            .chars()
            .last()
            .map(|s| u8::from_str(&s.to_string()))
            .transpose()?
        {
            if let Some(preset) = Preset::iter().nth((id - 1) as usize) {
                return Ok(preset);
            }
        }
        Err(anyhow!("Unable to Parse Preset from Number"))
    }

    pub fn mixer_mut(&mut self) -> &mut Mixers {
        &mut self.mixer
    }

    pub fn mixer(&self) -> &Mixers {
        &self.mixer
    }

    pub fn faders(&mut self) -> &mut EnumMap<Faders, Option<Fader>> {
        &mut self.faders
    }

    pub fn fader_mut(&mut self, fader: Faders) -> &mut Fader {
        self.faders[fader].as_mut().unwrap()
    }

    pub fn fader(&self, fader: Faders) -> &Fader {
        self.faders[fader].as_ref().unwrap()
    }

    pub fn mute_buttons(&mut self) -> &mut EnumMap<Faders, Option<MuteButton>> {
        &mut self.mute_buttons
    }

    pub fn mute_button_mut(&mut self, fader: Faders) -> &mut MuteButton {
        self.mute_buttons[fader].as_mut().unwrap()
    }

    pub fn mute_button(&self, fader: Faders) -> &MuteButton {
        self.mute_buttons[fader].as_ref().unwrap()
    }

    pub fn scribbles(&mut self) -> &mut EnumMap<Faders, Option<Scribble>> {
        &mut self.scribbles
    }

    pub fn scribble(&self, fader: Faders) -> &Scribble {
        self.scribbles[fader].as_ref().unwrap()
    }

    pub fn scribble_mut(&mut self, fader: Faders) -> &mut Scribble {
        self.scribbles[fader].as_mut().unwrap()
    }

    pub fn effects(&self, effect: Preset) -> &Effects {
        self.effects[effect].as_ref().unwrap()
    }

    pub fn effects_mut(&mut self, effect: Preset) -> &mut Effects {
        self.effects[effect].as_mut().unwrap()
    }

    pub fn mute_chat_mut(&mut self) -> &mut MuteChat {
        &mut self.mute_chat
    }

    pub fn mute_chat(&self) -> &MuteChat {
        &self.mute_chat
    }

    pub fn megaphone_effect(&self) -> &MegaphoneEffectBase {
        &self.megaphone_effect
    }

    pub fn megaphone_effect_mut(&mut self) -> &mut MegaphoneEffectBase {
        &mut self.megaphone_effect
    }

    pub fn robot_effect(&self) -> &RobotEffectBase {
        &self.robot_effect
    }

    pub fn robot_effect_mut(&mut self) -> &mut RobotEffectBase {
        &mut self.robot_effect
    }

    pub fn hardtune_effect(&self) -> &HardtuneEffectBase {
        &self.hardtune_effect
    }

    pub fn hardtune_effect_mut(&mut self) -> &mut HardtuneEffectBase {
        &mut self.hardtune_effect
    }

    pub fn sample_button(&self, button: SampleButtons) -> &SampleBase {
        self.sampler_map[button].as_ref().unwrap()
    }

    pub fn sample_button_mut(&mut self, button: SampleButtons) -> &mut SampleBase {
        self.sampler_map[button].as_mut().unwrap()
    }

    pub fn pitch_encoder(&self) -> &PitchEncoderBase {
        &self.pitch_encoder
    }

    pub fn pitch_encoder_mut(&mut self) -> &mut PitchEncoderBase {
        &mut self.pitch_encoder
    }

    pub fn echo_encoder(&self) -> &EchoEncoderBase {
        &self.echo_encoder
    }

    pub fn echo_encoder_mut(&mut self) -> &mut EchoEncoderBase {
        &mut self.echo_encoder
    }

    pub fn gender_encoder(&self) -> &GenderEncoderBase {
        &self.gender_encoder
    }

    pub fn gender_encoder_mut(&mut self) -> &mut GenderEncoderBase {
        &mut self.gender_encoder
    }

    pub fn reverb_encoder(&self) -> &ReverbEncoderBase {
        &self.reverb_encoder
    }

    pub fn reverb_encoder_mut(&mut self) -> &mut ReverbEncoderBase {
        &mut self.reverb_encoder
    }

    pub fn simple_element_mut(&mut self, name: SimpleElements) -> &mut SimpleElement {
        if self.simple_elements[name].is_some() {
            return self.simple_elements[name].as_mut().unwrap();
        }

        // If for whatever reason, this is missing, we'll use the global colour.
        return self.simple_elements[SimpleElements::GlobalColour]
            .as_mut()
            .unwrap();
    }

    pub fn simple_element(&self, name: SimpleElements) -> &SimpleElement {
        if self.simple_elements[name].is_some() {
            return self.simple_elements[name].as_ref().unwrap();
        }

        // If for whatever reason, this is missing, we'll use the global colour.
        return self.simple_elements[SimpleElements::GlobalColour]
            .as_ref()
            .unwrap();
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }
}
