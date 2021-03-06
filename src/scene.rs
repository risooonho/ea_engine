use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};

use crate::{
    component::{
        Background, Boss, CalculateOutOfBounds, ChangeSprite, Enemy, Fireball, Healing, Hero,
        Label, Position, Render, Shooter, Velocity,
    },
    enemy::BossConfig,
    entity_factory::{EntityFactory, EntityFactoryConfig},
    hero::HeroConfig,
    music::MusicPlayer,
    resources::{
        GameStateFlag, GameStateFlagRes, KeyboardKeys, LabelVariable, PressedKeys,
        VariableDictionary,
    },
    system::{
        CollisionSystem, FireballSystem, HeroBlinkingSystem, HeroControlSystem, LabelRenderSystem,
        OutOfBoundsSystem, RenderSystem, WalkSystem,
    },
};

use quicksilver::{graphics::Atlas, prelude::*};

use specs::prelude::*;

#[derive(PartialEq, Copy, Clone)]
enum GameState {
    WaitingInput,
    Initialiazing,
    Running,
    Paused,
    GameOver,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SceneConfig {
    pub atlas: String,
    pub font: String,
    pub main_background: String,
    pub defeat_background: String,
    pub victory_background: String,
    pub hero_config: HeroConfig,
    pub boss_config: BossConfig,
    pub entity_factory_config: EntityFactoryConfig,
    pub boss_cycle: u32,
    pub new_body_cycle: u64,
    pub normal_music: String,
    pub boss_music: String,
    pub game_over_music: String,
    pub victory_music: String,
}

impl Default for SceneConfig {
    fn default() -> SceneConfig {
        SceneConfig {
            atlas: "evil_alligator.atlas".to_string(),
            font: "cmunrm.ttf".to_string(),
            main_background: "cenario".to_string(),
            defeat_background: "inferno".to_string(),
            victory_background: "ceu".to_string(),
            hero_config: HeroConfig::default(),
            boss_config: BossConfig::default(),
            entity_factory_config: EntityFactoryConfig::default(),
            boss_cycle: 11,
            new_body_cycle: 210,
            normal_music: "music/normal.ogg".to_string(),
            boss_music: "music/boss.ogg".to_string(),
            game_over_music: "music/gameover.ogg".to_string(),
            victory_music: "music/victory.ogg".to_string(),
        }
    }
}

pub struct Scene {
    world: World,
    atlas: Rc<RefCell<Asset<Atlas>>>,
    font: Rc<RefCell<Asset<Font>>>,
    hero: Entity,
    state: GameState,
    cycle_timer: u64,
    cycle_counter: u32,
    music_player: MusicPlayer,
    entity_factory: EntityFactory,
    config: SceneConfig,
}

impl Scene {
    pub fn new(config: SceneConfig) -> Result<Self> {
        let atlas = Rc::new(RefCell::new(Asset::new(Atlas::load(config.atlas.clone()))));
        let font = Rc::new(RefCell::new(Asset::new(Font::load(config.font.clone()))));
        let music_player = MusicPlayer::new()?;

        let mut world = World::new();
        register_components(&mut world);
        add_resorces(&mut world);

        create_background(&mut world, config.main_background.clone());
        create_label(
            &mut world,
            LabelVariable::FramesPerSecond,
            FontStyle::new(48.0, Color::BLACK),
            Vector::new(20, 587),
        );
        create_label(
            &mut world,
            LabelVariable::HeroLives,
            FontStyle::new(48.0, Color::BLACK),
            Vector::new(10, 20),
        );
        create_label(
            &mut world,
            LabelVariable::Score,
            FontStyle::new(48.0, Color::BLACK),
            Vector::new(730, 20),
        );
        create_label(
            &mut world,
            LabelVariable::EngineVersion,
            FontStyle::new(48.0, Color::BLACK),
            Vector::new(730, 587),
        );
        let hero = crate::hero::create_hero(&mut world, config.hero_config.clone());

        Ok(Scene {
            world,
            atlas,
            font,
            hero,
            state: GameState::WaitingInput,
            cycle_timer: 0,
            cycle_counter: 0,
            music_player,
            entity_factory: EntityFactory::new(config.entity_factory_config.clone())?,
            config,
        })
    }

    pub fn update(&mut self, _window: &mut Window) -> Result<()> {
        if self.state != GameState::WaitingInput {
            if self.state == GameState::Running {
                self.entity_factory()?;
                self.run_update_systems()?;
                let flag = self.world.read_resource::<GameStateFlagRes>().flag;
                if let Some(f) = flag {
                    match f {
                        GameStateFlag::Victory => self.victory(),
                        GameStateFlag::Defeat => self.defeat(),
                    }?;
                }
            }
            self.music_player.update()?;
            self.world.maintain();
        }
        Ok(())
    }

    pub fn draw(&mut self, window: &mut Window) -> Result<()> {
        window.clear(Color::WHITE)?;

        let loaded_assets = self.has_loaded_atlas()? && self.has_loaded_font()?;
        if !loaded_assets {
            return Ok(());
        } else if loaded_assets && self.state == GameState::WaitingInput {
            return self.font.borrow_mut().execute(|font| {
                let rendered_label = font.render(
                    "Press ENTER to start...",
                    &FontStyle::new(72.0, Color::BLACK),
                )?;
                window.draw(
                    &rendered_label.area().with_center((400, 300)),
                    Img(&rendered_label),
                );
                Ok(())
            });
        } else if loaded_assets && self.state == GameState::Initialiazing {
            log::debug!("Starting game...");
            self.state = GameState::Running;
        }

        RenderSystem::new(window, Rc::clone(&self.atlas))?.run_now(&self.world.res);
        if self.state == GameState::Running {
            self.update_labels(window)?;
        }
        if self.state == GameState::Running || self.state == GameState::Paused {
            LabelRenderSystem::new(window, Rc::clone(&self.font))?.run_now(&self.world.res);
        }
        self.world.maintain();
        Ok(())
    }

    pub fn event(&mut self, event: &Event, window: &mut Window) -> Result<()> {
        match self.state {
            GameState::WaitingInput => match event {
                Event::Key(Key::Return, ButtonState::Pressed) => {
                    self.state = GameState::Initialiazing;
                }
                _ => {}
            },
            GameState::Running | GameState::Paused => {
                let mut pressed_keys = self.world.write_resource::<PressedKeys>();
                let pressed_keys = &mut pressed_keys.pressed_keys;
                match event {
                    Event::Key(Key::Up, ButtonState::Pressed)
                    | Event::Key(Key::W, ButtonState::Pressed)
                    | Event::GamepadButton(_, GamepadButton::DpadUp, ButtonState::Pressed) => {
                        pressed_keys.add(KeyboardKeys::KeyUp as u32);
                    }
                    Event::Key(Key::Up, ButtonState::Released)
                    | Event::Key(Key::W, ButtonState::Released)
                    | Event::GamepadButton(_, GamepadButton::DpadUp, ButtonState::Released) => {
                        pressed_keys.remove(KeyboardKeys::KeyUp as u32);
                    }
                    Event::Key(Key::Left, ButtonState::Pressed)
                    | Event::Key(Key::A, ButtonState::Pressed)
                    | Event::GamepadButton(_, GamepadButton::DpadLeft, ButtonState::Pressed) => {
                        pressed_keys.add(KeyboardKeys::KeyLeft as u32);
                    }
                    Event::Key(Key::Left, ButtonState::Released)
                    | Event::Key(Key::A, ButtonState::Released)
                    | Event::GamepadButton(_, GamepadButton::DpadLeft, ButtonState::Released) => {
                        pressed_keys.remove(KeyboardKeys::KeyLeft as u32);
                    }
                    Event::Key(Key::Right, ButtonState::Pressed)
                    | Event::Key(Key::D, ButtonState::Pressed)
                    | Event::GamepadButton(_, GamepadButton::DpadRight, ButtonState::Pressed) => {
                        pressed_keys.add(KeyboardKeys::KeyRight as u32);
                    }
                    Event::Key(Key::Right, ButtonState::Released)
                    | Event::Key(Key::D, ButtonState::Released)
                    | Event::GamepadButton(_, GamepadButton::DpadRight, ButtonState::Released) => {
                        pressed_keys.remove(KeyboardKeys::KeyRight as u32);
                    }
                    Event::Key(Key::P, ButtonState::Pressed)
                    | Event::Key(Key::Pause, ButtonState::Pressed)
                    | Event::GamepadButton(_, GamepadButton::Start, ButtonState::Pressed) => {
                        if self.state == GameState::Running {
                            self.state = GameState::Paused;
                        } else if self.state == GameState::Paused {
                            self.state = GameState::Running;
                        }
                    }
                    _ => {}
                };

                if let Event::Key(Key::Escape, ButtonState::Pressed) = event {
                    let mut flag = self.world.write_resource::<GameStateFlagRes>();
                    *flag = GameStateFlagRes {
                        flag: Some(GameStateFlag::Defeat),
                    };
                }
            }
            GameState::GameOver => {
                if let Event::Key(Key::Escape, ButtonState::Pressed)
                | Event::Key(Key::Return, ButtonState::Pressed)
                | Event::GamepadButton(_, GamepadButton::Start, ButtonState::Pressed) = event
                {
                    log::debug!("Closing window");
                    window.close();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn run_update_systems(&mut self) -> Result<()> {
        HeroControlSystem.run_now(&self.world.res);
        WalkSystem.run_now(&self.world.res);
        FireballSystem.run_now(&self.world.res);
        CollisionSystem.run_now(&self.world.res);
        OutOfBoundsSystem.run_now(&self.world.res);
        HeroBlinkingSystem.run_now(&self.world.res);
        Ok(())
    }

    fn entity_factory(&mut self) -> Result<()> {
        if self.cycle_counter < self.config.boss_cycle {
            if self.cycle_timer == 0 {
                self.music_player
                    .play_music(self.config.normal_music.clone())?;
            }
            self.cycle_timer += 1;
            if self.cycle_timer % self.config.new_body_cycle == 0 {
                self.cycle_counter += 1;
                if self.cycle_counter == self.config.boss_cycle {
                    self.music_player
                        .play_music(self.config.boss_music.clone())?;
                    crate::enemy::create_boss(&mut self.world, self.config.boss_config.clone());
                } else {
                    self.entity_factory.create_entity(&mut self.world)?;
                }
            }
        }
        Ok(())
    }

    fn defeat(&mut self) -> Result<()> {
        log::debug!("Player has been defeated");
        self.end_game()?;
        create_background(&mut self.world, self.config.defeat_background.clone());
        self.music_player
            .play_music(self.config.game_over_music.clone())?;
        Ok(())
    }

    fn victory(&mut self) -> Result<()> {
        log::debug!("Player is victorious");
        self.end_game()?;
        create_background(&mut self.world, self.config.victory_background.clone());
        self.music_player
            .play_music(self.config.victory_music.clone())?;
        Ok(())
    }

    fn end_game(&mut self) -> Result<()> {
        self.world.delete_all();
        self.state = GameState::GameOver;
        Ok(())
    }

    fn update_labels(&mut self, window: &Window) -> Result<()> {
        let hero_storage = self.world.read_storage::<Hero>();
        if let Some(hero) = hero_storage.get(self.hero) {
            let mut dict = self.world.write_resource::<VariableDictionary>();
            *dict = VariableDictionary {
                dictionary: [
                    (
                        LabelVariable::FramesPerSecond,
                        format!("{:.0}", window.average_fps()),
                    ),
                    (LabelVariable::HeroLives, format!("{}", hero.lives)),
                    (LabelVariable::Score, format!("{}", hero.score)),
                    (
                        LabelVariable::EngineVersion,
                        format!("v{}", env!("CARGO_PKG_VERSION")),
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }
        }
        Ok(())
    }

    fn has_loaded_atlas(&mut self) -> Result<bool> {
        let mut loaded_atlas =
            self.state != GameState::WaitingInput && self.state != GameState::Initialiazing;
        if !loaded_atlas {
            self.atlas.borrow_mut().execute(|_| {
                loaded_atlas = true;
                Ok(())
            })?;
        }
        Ok(loaded_atlas)
    }

    fn has_loaded_font(&mut self) -> Result<bool> {
        let mut loaded_font =
            self.state != GameState::WaitingInput && self.state != GameState::Initialiazing;
        if !loaded_font {
            self.font.borrow_mut().execute(|_| {
                loaded_font = true;
                Ok(())
            })?;
        }
        Ok(loaded_font)
    }
}

fn register_components(world: &mut World) {
    world.register::<Position>();
    world.register::<Velocity>();
    world.register::<Render>();
    world.register::<Shooter>();
    world.register::<Label>();
    world.register::<Hero>();
    world.register::<Boss>();
    world.register::<ChangeSprite>();
    world.register::<Enemy>();
    world.register::<Healing>();
    world.register::<Background>();
    world.register::<CalculateOutOfBounds>();
    world.register::<Fireball>();
}

fn add_resorces(world: &mut World) {
    world.add_resource(GameStateFlagRes { flag: None });
    world.add_resource(VariableDictionary {
        dictionary: [
            (LabelVariable::FramesPerSecond, "60".to_string()),
            (LabelVariable::HeroLives, "5".to_string()),
            (LabelVariable::Score, "0".to_string()),
            (
                LabelVariable::EngineVersion,
                format!("v{}", env!("CARGO_PKG_VERSION")),
            ),
        ]
        .iter()
        .cloned()
        .collect(),
    });
    world.add_resource(PressedKeys {
        pressed_keys: BitSet::new(),
    });
}

fn create_background(world: &mut World, sprite: String) -> Entity {
    world
        .create_entity()
        .with(Background)
        .with(Position {
            position: Vector::new(400, 300),
        })
        .with(Render {
            sprite,
            bounding_box: None,
        })
        .build()
}

fn create_label(
    world: &mut World,
    variable: LabelVariable,
    font_style: FontStyle,
    position: Vector,
) -> Entity {
    world
        .create_entity()
        .with(Label {
            bind_variable: variable,
            font_style,
        })
        .with(Position { position })
        .build()
}
