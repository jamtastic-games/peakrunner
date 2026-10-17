//! Lists the gamepads the game sees and their stick values for 3 seconds.
fn main() {
    let mut g = gilrs::Gilrs::new().expect("gilrs");
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f32() < 3.0 { while g.next_event().is_some() {} std::thread::sleep(std::time::Duration::from_millis(50)); }
    let mut n = 0;
    for (id, gp) in g.gamepads() {
        n += 1;
        println!("{id:?} {:?} connected {} ", gp.name(), gp.is_connected());
        for a in [gilrs::Axis::LeftStickX, gilrs::Axis::LeftStickY, gilrs::Axis::RightStickX, gilrs::Axis::RightStickY] {
            println!("   {a:?} = {:.3}", gp.value(a));
        }
    }
    println!("{n} gamepad(s)");
}
