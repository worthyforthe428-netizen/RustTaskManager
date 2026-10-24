// Put this file at src/main.rs next to Cargo.toml.
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Gauge, Row, Table, TableState},
    DefaultTerminal, Frame,
};
use std::{
    io,
    time::{Duration, Instant},
};
use sysinfo::{Pid, System};

const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, PartialEq)]
enum SortBy {
    Cpu,
    Mem,
}

struct ProcRow {
    pid: Pid,
    name: String,
    cpu: f32,
    mem: u64, // bytes
}

struct App {
    sys: System,
    rows: Vec<ProcRow>,
    sort: SortBy,
    state: TableState,
    quit: bool
}

impl App {
    fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let mut app = App {
            sys,
            rows: Vec::new(),
            sort: SortBy::Cpu,
            state: TableState::default().with_selected(Some(0)),
            quit: false,
        };
        app.rebuild_rows();
        app
    }
    
    fn refresh(&mut self) {
        self.sys.refresh_cpu();
        self.sys.refresh_memory();
        self.sys.refresh_processes();
        self.rebuild_rows();
    }

    fn rebuild_rows(&mut self) {
        self.rows = self
            .sys
            .processes()
            .iter()
            .filter(|(_, p)| p.thread_kind().is_none()
            .map(|(pid, p)| ProcRow {
                pid: *pid,
                name: p.name().to_string(),
                cpu: p.cpu_usage(),
                mem: p.memory(), 
            })
            .collect();
        match self.sort {
            SortBy::Cpu => self.rows.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
            SortBy::Mem => self.rows.sort_by(|a, b| b.mem.cmp(&a.mem)),
        }

        // keep the selection inside the list 
        let max = self.rows.len().saturating_sub(1);
        let sel = self.state.selected().unwrap_or(0).min(max);
        self.state.select(Some(sel));
    }

    fn move_selection(&mut self, delta: isize) {
        let max = self.rows.len().saturating_sub(1) as isize;
        let cur = self.state.selected().unwrap_or(0) as isize;
        self.state.select(Some((cur + delta).clamp(0, max) as usize));
    }

    fn kill_selected(&mut self) {
        if let Some(row) = self.state.selected().and_then(|i| self.rows.get(i)) {
            if let Some(p) = self.sys.process(row.pid) {
                p.kill();
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode:: Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('c') => {
                self.sort = SortBy::Cpu;
                self.rebuild_rows();
            }
            KeyCode::Char('m') => {
                self.sort = SortBy::Mem;
                self.rebuild_rows();
            }
            KeyCode::F(9) | KeyCode::Char('x') => self.kill_selected(),
            _ => {}
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let mut last_tick = Instant::now();
        while !self.quit {
            terminal.draw(|f| self.draw(f))?;
            let timeout = TICK.saturating_sub(last_tick.elapsed());

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }
            if last_tick.elapsed() >= TICK {
                self.refresh();
                last_tick = Instant::now();
            }
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [cpu_area, mem_area, table_area, help_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let cpu = self.sys.global_cpu_info().cpu_usage();
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title("CPU"))
                .gauge_style(Style::default().fg(Color::Green))
                .ratio((cpu as f64 / 100.0).clamp(0.0, 1.0))
                .label(format!("{cpu:.1}%")),
            cpu_area,
        );

        let used = self.sys.used_memory();
        let total = self.sys.total_memory().max(1);
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title("Memory"))
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio((used as f64 / total as f64).clamp(0.0, 1.0))
                .label(format!("{} / {} MB", used >> 20, total >> 20)),
            mem_area,
        );

        let header = Row::new(["PID", "NAME", "CPU %", "MEM"])
            .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow));
        let rows = self.rows.iter().map(|r| {
            Row::new([
                r.pid.to_string(),
                r.name.clone(),
                format!("{:.1}", r.cpu),
                format!("{} MB", r.mem >> 20),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Min(20),
                Constraint::Length(8),
                Constraint::Length(10),
            ],
        )
            .header(header)
            .block(Block::bordered().title(format!("Processes({}", self.rows.len())))
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
            frame.render_stateful_widget(table, table_area, &mut self.state);

        frame.render_widget(
            ratatui::widgets::Paragraph::new(
                "q quit | ↑/↓ select | c sort CPU | m sort MEM | x kill",),
            help_area,
        );
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = App::new().run(&mut terminal);
    ratatui::restore();
    result
}
