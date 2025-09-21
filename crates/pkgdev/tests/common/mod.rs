use workspace::Workspace;

#[allow(dead_code)]
const PKG_TEST_WORKSPACE: &str = "sample_data/ws";

#[allow(dead_code)]
pub fn setup() {
    std::fs::create_dir_all(PKG_TEST_WORKSPACE).unwrap();
}

#[allow(dead_code)]
pub fn get_test_workspace() -> Workspace {
    Workspace::new(PKG_TEST_WORKSPACE).unwrap()
}
