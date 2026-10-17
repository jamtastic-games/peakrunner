//! The Library: how to play, in simple words with pictures from the game
//! itself (`assets/library/*.png`, captured in-game). Opened from the main
//! menu. Words in braces (`{jump}`) are replaced with the player's own keys.
use crate::keybinds::{Action, Keybinds};

/// One page of the Library.
pub struct Topic {
    pub title: &'static str,
    /// The line on its card.
    pub card: &'static str,
    pub picture: &'static str,
    pub parts: &'static [Part],
}

pub enum Part {
    /// A short paragraph.
    Text(&'static str),
    /// A thing with its own small picture: a weapon, a map, a pack.
    Item { picture: &'static str, name: &'static str, text: &'static str },
}

use Part::{Item, Text};

pub const TOPICS: &[Topic] = &[
    Topic { title: "Run, ski and fly", card: "How to get around fast.", picture: "moving", parts: &[
        Text("Use {move} to run around."),
        Text("On a hill, hold {jump} to ski. You slide down and go faster and faster, like a sled!"),
        Text("Hold the right mouse button to use your jetpack. It lifts you up but uses energy, the blue bar at the bottom. Let go and it fills back up."),
        Text("The trick the best players use: ski down one hill, then jet over the top of the next one."),
    ] },
    Topic { title: "Capture the Flag", card: "Grab their flag and bring it home.", picture: "flag", parts: &[
        Text("Each team has a flag at its base. Yours is the color of your team."),
        Text("Sneak or zoom to the other team's base, touch their flag to grab it, then bring it all the way back to your own flag."),
        Text("If you get tagged while you carry it, you drop it. A teammate can pick it up, or the other team can touch it to send it home."),
        Text("Being chased? Press {toss} to throw the flag to a friend!"),
        Text("The first team to bring back 3 flags wins."),
    ] },
    Topic { title: "Capture & Hold", card: "Make the towers your color.", picture: "hold", parts: &[
        Text("There are towers around the map. Stand close to one and it slowly turns your team's color."),
        Text("Every tower your team holds earns points. The first team to 300 points wins."),
        Text("Leave a friend near your towers, or the other team will sneak in and take them."),
    ] },
    Topic { title: "Team Deathmatch", card: "Tag the other team.", picture: "deathmatch", parts: &[
        Text("No flags here. Every time you tag someone on the other team, your team scores a point."),
        Text("Every round is a surprise: sunrise or night, rain, snow or fog, and sometimes a twist, like everybody in heavy armor!"),
    ] },
    Topic { title: "Rabbit", card: "Grab the flag and run!", picture: "flag", parts: &[
        Text("There's one gold flag. Whoever holds it is the rabbit, and everyone else is on the same side, hunting them. Hunters can't hurt each other."),
        Text("Grab the flag for a point, then keep it: every 15 seconds you hold it, adding up all your turns, is another point. Tagging the rabbit is a point, and every hunter the rabbit tags is two. Most points when time runs out wins."),
        Text("Every so often the hunters see where the rabbit is, unless someone has hit the rabbit lately. The rabbit can't throw the flag away, so keep moving!"),
    ] },
    Topic { title: "Team Rabbit", card: "The flag doubles your tags.", picture: "deathmatch", parts: &[
        Text("Each team starts with 50 lives, and every time someone on a team is tagged, that team loses one."),
        Text("The first time anyone is tagged, a gold flag falls where they were. While your team holds it, every tag takes two lives instead of one."),
        Text("Hold the flag with your team around you, or throw it to a friend before you go down."),
    ] },
    Topic { title: "Football", card: "Carry the ball into their end zone.", picture: "football", parts: &[
        Text("Grab the ball and run it into the other team's end zone to score a touchdown."),
        Text("Click to throw the ball to a teammate. Run into the player with the ball to tackle them."),
        Text("There are no weapons in Football, just running, jumping and passing."),
    ] },
    Topic { title: "Weapons", card: "What each one does.", picture: "weapons", parts: &[
        Text("Light armor carries 3 weapons, medium armor 4 and heavy armor 5. You choose them at an inventory station. Keys {slot1}, {slot2} and {slot3} pick your first, second and third weapon, and so on."),
        Item { picture: "w-disc", name: "Disc launcher", text: "Shoots a spinning disc that pops when it hits. Shoot it at your feet to jump really high (it stings a little)." },
        Item { picture: "w-chaingun", name: "Repeater", text: "Lots of fast little shots. Great up close." },
        Item { picture: "w-launcher", name: "Grenade launcher and mortar", text: "Lobs bouncy grenades over walls. In heavy armor it's the mortar: big, slow and very loud! A scale by the crosshair shows how high you're aiming; 45 throws farthest." },
        Item { picture: "w-rifle", name: "Laser rifle and railgun", text: "Hits things far away. Hold {use} to zoom in, and roll the mouse wheel to zoom in even more: the closer you zoom, the farther away you can see who people are. Only light armor has the laser, which uses energy. Only heavy armor has the railgun: its glowing slug takes a moment to get there, one hit knocks out light armor, and every shot uses up all your energy." },
        Item { picture: "w-plasma", name: "Plasma gun", text: "Shoots glowing orange balls that burst when they hit." },
        Item { picture: "w-blaster", name: "Blaster", text: "Fast purple zaps that use energy, so it never runs out. They even bounce off the ground!" },
        Item { picture: "w-drain", name: "Drain gun", text: "Hold it on someone from the other team to drain their energy, so they can't fly. It uses your energy too." },
        Item { picture: "w-marker", name: "Target marker", text: "Point it at someone from the other team and click. For 12 seconds your whole team can see where they are, even through walls!" },
    ] },
    Topic { title: "Armor and the inventory", card: "Choose what you carry.", picture: "inventory", parts: &[
        Text("Inside your base are inventory stations. Walk in from the front and press {use}."),
        Text("Light armor is quick and flies well. Medium armor is in between. Heavy armor is slow but very tough."),
        Text("Light armor can't carry turrets, walls, force fields or stations. Medium and heavy armor can carry every pack."),
        Text("Pick your armor, your weapons and a pack. When you come back into the game after being tagged, you'll have the same things."),
        Text("Save a setup as a favorite, then press F1 to F4 to get it all back at once."),
    ] },
    Topic { title: "Things you can build", card: "Turrets, walls and more.", picture: "packs", parts: &[
        Text("Take a pack at an inventory station. Press {deploy}, aim at the ground and click to put it there."),
        Item { picture: "p-turret", name: "Turret", text: "Shoots at the other team all by itself." },
        Item { picture: "p-wall", name: "Wall", text: "Blocks everybody and every shot." },
        Item { picture: "p-field", name: "Force field", text: "Your team can walk through it. The other team can't." },
        Item { picture: "p-ammo", name: "Ammo station", text: "Your team refills here." },
        Item { picture: "p-inventory", name: "Field inventory", text: "A little station you can put anywhere, to change weapons." },
        Text("Sensors: put one down, and your team sees enemies near it on their screens. A jammer pylon hides your team from sensors and turrets."),
        Text("Some packs you wear instead: an energy pack, a shield pack, a repair pack, a sensor jammer, or an ammo pack for more rounds. Press {deploy} to switch the shield, repair pack and jammer on and off. The shield and jammer use a little more energy than you get back, so they slowly empty your tank even when you stand still."),
    ] },
    Topic { title: "Grenades and mines", card: "Throw them by hand.", picture: "throw", parts: &[
        Text("Hold {grenade} to wind up and let go to throw a grenade. It goes boom 2 seconds later."),
        Text("Press {mine} to throw a mine. It waits on the ground until someone from the other team walks too close."),
    ] },
    Topic { title: "The maps", card: "Nine places to play.", picture: "maps", parts: &[
        Item { picture: "m-old-holler", name: "Old Holler", text: "Rainy green hills, a deep river canyon, and flags up in tall towers." },
        Item { picture: "m-tower-complex", name: "Tower Complex", text: "Towers floating in the sky over rolling hills." },
        Item { picture: "m-cairnhold", name: "Cairnhold", text: "Bunkers dug into the hills and a big stone ring in the middle." },
        Item { picture: "m-frostline", name: "Frostline", text: "Snowy mountains with ice stations on the cliffs. Watch out for the fog!" },
        Item { picture: "m-dustreach", name: "Dustreach", text: "Desert castles in the sand dunes, with a giant gate in the middle." },
        Item { picture: "m-ozarktic-blast", name: "Ozarktic Blast", text: "A long ridge, and two strange ships floating over a valley." },
        Item { picture: "m-reefbreak", name: "Reefbreak", text: "A ring of islands in the sea, with big ships floating over the water." },
        Item { picture: "m-longfield", name: "Longfield and Highgoal", text: "Stadiums for Football." },
    ] },
    Topic { title: "Day, night and weather", card: "Rain, snow, fog and stars.", picture: "sky", parts: &[
        Text("Some servers change the sky every round: sunrise, daytime, sunset or night, with rain, snow, fog or a storm."),
        Text("In Singleplayer you can pick the sky yourself, or let it surprise you."),
    ] },
    Topic { title: "Make your own", card: "Change the game with mods.", picture: "mods", parts: &[
        Text("A mod is a folder with a few small text files. It can make you jump higher, make discs fly faster, or change how many flags win."),
        Text("Anyone can run a server with mods. When you join one, the game asks before it downloads anything."),
        Text("Want to try making one? Look at peakrunner.net/docs/mods."),
    ] },
];

/// `{jump}`, `{use}`… in `text`, with the player's keys.
pub fn fill(text: &str, keys: &Keybinds) -> String {
    let moves = [Action::Forward, Action::Left, Action::Back, Action::Right].map(|a| keys.name(a)).join(", ");
    let mut out = text.replace("{move}", &moves);
    for (word, action) in [("{jump}", Action::Jump), ("{use}", Action::Use), ("{deploy}", Action::Deploy), ("{grenade}", Action::Grenade),
        ("{mine}", Action::Mine), ("{slot1}", Action::Slot1), ("{slot2}", Action::Slot2), ("{slot3}", Action::Slot3)] {
        out = out.replace(word, keys.name(action));
    }
    out.replace("{toss}", &format!("Ctrl+{}", keys.name(Action::TossFlag)))
}

/// The pictures, decoded into textures the first time they're shown.
#[derive(Default)]
pub struct Pictures { loaded: std::collections::HashMap<&'static str, Option<egui::TextureHandle>> }

impl Pictures {
    pub fn get(&mut self, ctx: &egui::Context, name: &'static str) -> Option<egui::TextureHandle> {
        self.loaded.entry(name).or_insert_with(|| {
            let image = decode(bytes(name)?)?;
            Some(ctx.load_texture(format!("library-{name}"), image, egui::TextureOptions::LINEAR))
        }).clone()
    }
}

/// A picture's PNG (none in the browser build, which keeps its download small).
#[cfg(target_arch = "wasm32")]
fn bytes(_name: &str) -> Option<&'static [u8]> { None }
#[cfg(target_arch = "wasm32")]
fn decode(_png: &[u8]) -> Option<egui::ColorImage> { None }

/// A picture's PNG.
#[cfg(not(target_arch = "wasm32"))]
fn bytes(name: &str) -> Option<&'static [u8]> {
    macro_rules! pictures { ($($n:literal),* $(,)?) => { match name { $($n => Some(include_bytes!(concat!("../assets/library/", $n, ".png"))),)* _ => None } } }
    pictures!("moving", "flag", "hold", "deathmatch", "football", "weapons", "inventory", "packs", "throw", "maps", "sky", "mods",
        "w-disc", "w-chaingun", "w-launcher", "w-rifle", "w-plasma", "w-blaster", "w-drain", "w-marker",
        "p-turret", "p-wall", "p-field", "p-ammo", "p-inventory",
        "m-old-holler", "m-tower-complex", "m-cairnhold", "m-frostline", "m-dustreach", "m-ozarktic-blast", "m-reefbreak", "m-longfield")
}

/// An 8-bit RGB, RGBA or paletted PNG as an egui image.
#[cfg(not(target_arch = "wasm32"))]
fn decode(png_bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        _ => return None,
    };
    Some(egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn every_picture_is_there_and_decodes_and_every_key_is_filled() {
        let mut names: Vec<&str> = TOPICS.iter().map(|t| t.picture).collect();
        for t in TOPICS { for p in t.parts { if let Item { picture, .. } = p { names.push(picture); } } }
        for name in names {
            let image = bytes(name).and_then(decode).unwrap_or_else(|| panic!("{name}"));
            assert!(image.size[0] >= 200 && image.size[1] >= 100, "{name}: {:?}", image.size);
        }
        let keys = Keybinds::default();
        for t in TOPICS {
            for p in t.parts {
                let text = match p { Text(t) => t, Item { text, .. } => text };
                assert!(!fill(text, &keys).contains('{'), "{text}");
            }
        }
    }
}
