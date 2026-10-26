use std::io;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            KeyboardInteractivity, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    backend::WaylandError,
    globals::{GlobalList, registry_queue_init},
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

use crate::art::{self, Art, Pose};
use crate::bubble::{self, Bubble};
use crate::config::{Config, Corner};
use crate::cpu::Cpu;
use crate::pet::Pet;
use crate::sprite::BYTES_PER_PIXEL;

const NAMESPACE: &str = "toothless";
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
/// How long his speech bubble stays up.
const BUBBLE_TIME: Duration = Duration::from_secs(5);

/// The layer surface he lives on, on one output.
struct Den {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    configured: bool,
    buffer_scale: i32,
}

pub struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    layer_shell: LayerShell,

    config: Config,
    art: Art,
    bubble: Bubble,
    pet: Pet,
    cpu: Cpu,
    /// The latest CPU reading, once there is one.
    usage: Option<f32>,
    den: Option<Den>,
    pool: Option<SlotPool>,
    buffer: Option<Buffer>,
    pointer: Option<wl_pointer::WlPointer>,

    pose: Pose,
    /// The pose, what his bubble said if he had one up, and the buffer scale.
    drawn: Option<(Pose, Option<String>, i32)>,
    next_frame: Instant,
    next_sample: Instant,
    /// When his speech bubble pops, while he has one up.
    bubble_until: Option<Instant>,
}

impl App {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<Self>, config: Config) -> Result<Self> {
        let compositor = CompositorState::bind(globals, qh).context("wl_compositor unavailable")?;
        let shm = Shm::bind(globals, qh).context("wl_shm unavailable")?;
        let layer_shell = LayerShell::bind(globals, qh)
            .context("wlr-layer-shell unavailable, this compositor cannot host toothless")?;

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |t| t.subsec_nanos());
        let mut pet = Pet::new(config.thresholds(), seed);
        let (pose, _) = pet.advance();

        let now = Instant::now();
        Ok(Self {
            registry_state: RegistryState::new(globals),
            seat_state: SeatState::new(globals, qh),
            output_state: OutputState::new(globals, qh),
            shm,
            compositor,
            layer_shell,
            art: Art::load()?,
            bubble: Bubble::load()?,
            pet,
            cpu: Cpu::default(),
            usage: None,
            config,
            den: None,
            pool: None,
            buffer: None,
            pointer: None,
            pose,
            drawn: None,
            next_frame: now,
            next_sample: now,
            bubble_until: None,
        })
    }

    fn output_name(&self, output: &wl_output::WlOutput) -> Option<String> {
        self.output_state.info(output).and_then(|info| info.name)
    }

    fn settle(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.den.is_some() {
            return;
        }
        let name = self.output_name(&output);
        if let Some(wanted) = &self.config.output
            && name.as_ref() != Some(wanted)
        {
            return;
        }

        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            self.config.layer,
            Some(NAMESPACE),
            Some(&output),
        );

        let scale = self.config.scale;
        let margin = self.config.margin;
        let corner = self.config.corner;
        let (top, bottom) = match corner {
            Corner::TopLeft | Corner::TopRight => (margin, 0),
            Corner::BottomLeft | Corner::BottomRight => (0, margin),
        };
        let (left, right) = if corner.is_right() {
            (0, margin)
        } else {
            (margin, 0)
        };

        layer.set_anchor(corner.anchor());
        // he takes the rows nearest his corner, and his speech bubble the strip past them
        layer.set_size(
            art::WIDTH as u32 * scale,
            (art::HEIGHT + bubble::HEIGHT) as u32 * scale,
        );
        layer.set_margin(top, right, bottom, left);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        // clicks land on him, not on the empty strip his bubble pops up in
        match Region::new(&self.compositor) {
            Ok(region) => {
                let scale = scale as i32;
                let rows_above = if corner.is_top() {
                    0
                } else {
                    bubble::HEIGHT as i32
                };
                region.add(
                    0,
                    rows_above * scale,
                    art::WIDTH as i32 * scale,
                    art::HEIGHT as i32 * scale,
                );
                layer
                    .wl_surface()
                    .set_input_region(Some(region.wl_region()));
            }
            Err(err) => log::error!("failed to create an input region: {err}"),
        }
        layer.commit();

        log::info!(
            "settling on output {}",
            name.as_deref().unwrap_or("<unnamed>")
        );
        self.den = Some(Den {
            output,
            layer,
            configured: false,
            buffer_scale: 1,
        });
        self.drawn = None;
    }

    fn leave(&mut self) {
        self.den = None;
        self.buffer = None;
        self.pool = None;
        self.drawn = None;
    }

    fn draw(&mut self) {
        let Some(den) = &self.den else { return };
        let said = self.bubble_until.map(|_| bubble::cpu(self.usage));
        let drawing = (self.pose, said, den.buffer_scale);
        if !den.configured || self.drawn.as_ref() == Some(&drawing) {
            return;
        }

        let scale = self.config.scale as usize * den.buffer_scale as usize;
        let (width, height) = (art::WIDTH * scale, (art::HEIGHT + bubble::HEIGHT) * scale);
        let stride = width * BYTES_PER_PIXEL;

        let pool = match &mut self.pool {
            Some(pool) => pool,
            None => match SlotPool::new(stride * height * 2, &self.shm) {
                Ok(pool) => self.pool.insert(pool),
                Err(err) => {
                    log::error!("failed to create an shm pool: {err}");
                    return;
                }
            },
        };

        // reuse the last buffer once the compositor has let go of it and it is still the right size
        let reusable = self
            .buffer
            .take()
            .filter(|b| b.height() == height as i32 && b.stride() == stride as i32)
            .filter(|b| pool.canvas(b).is_some());
        let buffer = match reusable {
            Some(buffer) => buffer,
            None => match pool.create_buffer(
                width as i32,
                height as i32,
                stride as i32,
                wl_shm::Format::Argb8888,
            ) {
                Ok((buffer, _)) => buffer,
                Err(err) => {
                    log::error!("failed to create a buffer: {err}");
                    return;
                }
            },
        };
        let Some(canvas) = pool.canvas(&buffer) else {
            return;
        };

        let mirror = self.config.mirrored();
        let below = self.config.corner.is_top();
        let rows_above = if below { art::HEIGHT } else { bubble::HEIGHT };
        let (above, beneath) = canvas.split_at_mut(rows_above * scale * stride);
        let (frame, strip) = if below {
            (above, beneath)
        } else {
            (beneath, above)
        };
        self.art.render(self.pose).blit(frame, scale, mirror);
        match &drawing.1 {
            Some(text) => self
                .bubble
                .render(text, below, mirror)
                .blit(strip, scale, false),
            None => strip.fill(0),
        }

        let surface = den.layer.wl_surface();
        surface.set_buffer_scale(den.buffer_scale);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(err) = buffer.attach_to(surface) {
            log::error!("failed to attach buffer: {err}");
            return;
        }
        den.layer.commit();

        self.drawn = Some(drawing);
        self.buffer = Some(buffer);
    }

    /// Runs whatever is due and returns when to wake up next.
    fn tick(&mut self) -> Instant {
        let now = Instant::now();

        if now >= self.next_sample {
            if let Some(cpu) = self.cpu.sample() {
                self.usage = Some(cpu);
                let before = self.pet.mood();
                self.pet.set_cpu(cpu);
                if self.pet.mood() != before {
                    log::debug!("cpu at {cpu:.0}%, now {:?}", self.pet.mood());
                    self.next_frame = now;
                }
            }
            self.next_sample = now + Duration::from_millis(self.config.sample_interval);
        }

        if self.bubble_until.is_some_and(|until| now >= until) {
            self.bubble_until = None;
        }

        if now >= self.next_frame {
            let (pose, hold) = self.pet.advance();
            self.pose = pose;
            self.next_frame = now + hold;
        }
        // does nothing unless his pose, his bubble or the reading in it changed
        self.draw();

        let next = self.next_frame.min(self.next_sample);
        self.bubble_until.map_or(next, |until| next.min(until))
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let Some(den) = &mut self.den else { return };
        if den.layer.wl_surface() == surface && den.buffer_scale != new_factor {
            den.buffer_scale = new_factor.max(1);
            self.draw();
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.settle(qh, output);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if self.den.as_ref().is_some_and(|den| den.output == output) {
            log::info!("his output went away, looking for another");
            self.leave();
            let others: Vec<_> = self
                .output_state
                .outputs()
                .filter(|o| *o != output)
                .collect();
            for other in others {
                self.settle(qh, other);
            }
        }
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.den.as_ref().is_some_and(|den| den.layer == *layer) {
            log::info!("the compositor closed his surface");
            self.leave();
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(den) = &mut self.den else { return };
        if den.layer == *layer {
            den.configured = true;
            self.drawn = None;
            self.draw();
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(err) => log::error!("failed to acquire pointer: {err}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let now = Instant::now();
        for event in events {
            if !self
                .den
                .as_ref()
                .is_some_and(|den| den.layer.wl_surface() == &event.surface)
            {
                continue;
            }
            match event.kind {
                PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } => {
                    self.pet.poke();
                    self.next_frame = now;
                }
                // he says how busy the CPU is, until it pops or he's right-clicked again
                PointerEventKind::Press {
                    button: BTN_RIGHT, ..
                } => {
                    self.bubble_until = match self.bubble_until {
                        Some(_) => None,
                        None => Some(now + BUBBLE_TIME),
                    };
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);
delegate_dispatch2!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

/// Waits for wayland events until `deadline`, then dispatches whatever arrived.
fn wait(event_queue: &mut EventQueue<App>, app: &mut App, deadline: Instant) -> Result<()> {
    event_queue.flush().context("wayland flush failed")?;

    if let Some(guard) = event_queue.prepare_read() {
        let timeout = deadline.saturating_duration_since(Instant::now());
        // round up, so a deadline less than a millisecond away doesn't spin
        let millis = timeout.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32;
        let mut fd = libc::pollfd {
            fd: guard.connection_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd, and the guard keeps the connection's fd open
        let ready = unsafe { libc::poll(&mut fd, 1, millis) };
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                return Err(err).context("poll failed");
            }
        } else if ready > 0 {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => return Err(err).context("reading wayland events failed"),
            }
        }
    }

    event_queue
        .dispatch_pending(app)
        .context("wayland dispatch failed")?;
    Ok(())
}

pub fn run(config: Config) -> Result<()> {
    let conn = Connection::connect_to_env()
        .context("could not connect to a wayland compositor, is WAYLAND_DISPLAY set?")?;

    let (globals, mut event_queue) =
        registry_queue_init(&conn).context("failed to enumerate wayland globals")?;
    let qh = event_queue.handle();

    let mut app = App::new(&globals, &qh, config)?;

    event_queue
        .roundtrip(&mut app)
        .context("initial roundtrip failed")?;

    if app.den.is_none() {
        match &app.config.output {
            Some(name) => log::info!("waiting for output {name}"),
            None => log::warn!("no outputs yet, waiting for one to appear"),
        }
    }

    loop {
        let deadline = app.tick();
        wait(&mut event_queue, &mut app, deadline)?;
    }
}
