use stuie::app::delete_previous_word;

#[test]
fn delete_previous_word_removes_trailing_whitespace_then_word() {
    let mut s = String::from("hello world   ");
    delete_previous_word(&mut s);
    assert_eq!(s, "hello ");
}

#[test]
fn delete_previous_word_stops_at_path_separator() {
    let mut s = String::from("/home/user/file.txt");
    delete_previous_word(&mut s);
    assert_eq!(s, "/home/user/");
}

#[test]
fn delete_previous_word_on_empty_string_is_noop() {
    let mut s = String::new();
    delete_previous_word(&mut s);
    assert_eq!(s, "");
}

#[test]
fn delete_previous_word_single_word() {
    let mut s = String::from("word");
    delete_previous_word(&mut s);
    assert_eq!(s, "");
}

#[test]
fn delete_previous_word_handles_windows_separator() {
    let mut s = String::from("C:\\Users\\Name");
    delete_previous_word(&mut s);
    assert_eq!(s, "C:\\Users\\");
}
