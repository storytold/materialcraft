pub(crate) fn inspected<T: Copy>(selection: &[T], previous: Option<T>) -> Option<T> {
    selection.last().copied().or(previous)
}
