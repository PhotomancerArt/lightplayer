pub trait HwDriver {
    fn driver_id(&self) -> &str;

    fn display_label(&self) -> &str {
        self.driver_id()
    }
}
