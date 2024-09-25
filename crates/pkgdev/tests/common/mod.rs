use workspace::Workspace;

const PKG_TEST_WORKSPACE: &str = "sample_data/ws";

pub fn setup() {
    std::fs::create_dir_all(PKG_TEST_WORKSPACE).unwrap();
}

pub fn get_test_workspace() -> Workspace {
    Workspace::new(PKG_TEST_WORKSPACE).unwrap()
}
