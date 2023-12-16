// use std::collections::HashMap;

pub trait Actuator {
    #[must_use]
    async fn connected(&mut self);

    #[must_use]
    async fn disconnected(&mut self);

    #[must_use]
    async fn get_screen_size(&self) -> (u16, u16);

    #[must_use]
    async fn get_cursor_position(&self) -> (u16, u16);

    #[must_use]
    async fn set_cursor_position(&mut self, x: u16, y: u16);

    #[must_use]
    async fn move_cursor(&mut self, x: i16, y: i16) {
        let (cx, cy) = self.get_cursor_position().await;
        self.set_cursor_position((cx as i32 + x as i32) as u16, (cy as i32 + y as i32) as u16)
            .await;
    }

    #[must_use]
    async fn mouse_down(&mut self, button: i8);

    #[must_use]
    async fn mouse_up(&mut self, button: i8);

    #[must_use]
    async fn mouse_wheel(&mut self, x: i16, y: i16);

    #[must_use]
    async fn key_down(&mut self, key: u16, mask: u16, button: u16);

    #[must_use]
    async fn key_repeat(&mut self, key: u16, mask: u16, button: u16, count: u16);

    #[must_use]
    async fn key_up(&mut self, key: u16, mask: u16, button: u16);

    #[must_use]
    async fn reset_options(&mut self);

    #[must_use]
    async fn enter(&mut self);

    #[must_use]
    async fn leave(&mut self);
}
