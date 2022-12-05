use console::StyledObject;

pub trait ConsoleStyleable<T> {
    fn styled(self) -> StyledObject<T>;
    fn path_styled(self) -> StyledObject<T>;
    fn tag_styled(self) -> StyledObject<T>;
    fn error_styled(self) -> StyledObject<T>;
}

impl<T> ConsoleStyleable<T> for T {
    fn styled(self) -> StyledObject<T> {
        console::style(self)
    }

    fn path_styled(self) -> StyledObject<T> {
        self.styled().bold()
    }

    fn tag_styled(self) -> StyledObject<T> {
        self.styled().bold()
    }

    fn error_styled(self) -> StyledObject<T> {
        self.styled().bold().red()
    }
}
