#![feature(type_alias_impl_trait)]
use edge_executor::{Local, Task};
use embassy_futures::select::{select, Either};
use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_sys::EspError;

use esp_idf_hal::executor::EspExecutor;
use esp_idf_hal::gpio::{Input, InputPin, InterruptType, OutputPin, PinDriver};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::peripherals::Peripherals;

//se esp_idf_sys::EspError;
use static_cell::StaticCell;

mod notification;
use notification::*;

fn main() {
    esp_idf_hal::cs::critical_section::link();
    esp_idf_svc::timer::embassy_time::driver::link();
    esp_idf_svc::timer::embassy_time::queue::link();
    // Temporary. Will disappear once ESP-IDF 4.4 is released, but for now it is necessary to call this function once,
    // or else some patches to the runtime implemented by esp-idf-sys might not link properly.
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();
    let button_pin = peripherals.pins.gpio9;

    static SYSTEM: StaticCell<System> = StaticCell::new();
    let system = &*SYSTEM.init(System::new());

    let mut tasks = heapless::Vec::<Task<()>, 2>::new();
    let mut executor = EspExecutor::<2, Local>::new();

    executor
        .spawn_local_collect(
            system
                .worker1_job(subscribe_pin(button_pin, move || system.worker1_callback()).unwrap()),
            &mut tasks,
        )
        .unwrap();

    executor.with_context(|exec, ctx| exec.run_tasks(ctx, || true, tasks));
}

pub struct System {
    worker1: Notification,
}

impl System {
    pub fn new() -> Self {
        Self {
            worker1: Notification::new(),
        }
    }

    pub fn worker1_callback(&self) {
        self.worker1.notify();
    }

    pub async fn worker1_job(&'static self, _pin: impl embedded_hal::digital::v2::InputPin) {
        let mut pin_edge = NotifReceiver::new(&self.worker1, &NoopStateCell);
        loop {
            pin_edge.recv().await;
            println!("Start");
            loop {
                let tick = embassy_time::Timer::after(embassy_time::Duration::from_secs_floor(2));
                let event = pin_edge.recv();
                let result = select(tick, event).await;
                match result {
                    Either::First(_) => println!("Run further"),
                    Either::Second(_) => {
                        println!("Stop");
                        break;
                    }
                }
            }
        }
    }
}

fn subscribe_pin<'d, P: InputPin + OutputPin>(
    pin: impl Peripheral<P = P> + 'd,
    notify: impl Fn() + Send + 'static,
) -> Result<PinDriver<'d, P, Input>, EspError> {
    let mut pin = PinDriver::input(pin)?;

    pin.set_interrupt_type(InterruptType::NegEdge)?;

    unsafe {
        pin.subscribe(notify)?;
    }
    Ok(pin)
}
