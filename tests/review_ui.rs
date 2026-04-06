use bubblepolicy::common::{Access, PolicyEntry};
use bubblepolicy::review_ui::{ui, App};
use insta::assert_snapshot;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn create_test_app() -> App {
    let entries = vec![
        PolicyEntry {
            path: "/etc".to_string(),
            access: Access::ReadOnly,
        },
        PolicyEntry {
            path: "/tmp".to_string(),
            access: Access::Tmpfs,
        },
        PolicyEntry {
            path: "/tmp/test".to_string(),
            access: Access::ReadWrite,
        },
    ];
    App::from_entries(entries, "test.policy".to_string())
}

#[test]
fn test_render_tree() {
    let mut app = create_test_app();
    app.select_first();

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();

    assert_snapshot!(terminal.backend());
}
