use httpmock::Method::GET;
use httpmock::{Mock, MockRef, MockServer};
use serial_test::serial;

mod common;

use goose::goose::GooseTaskSet;
use goose::prelude::*;
use goose::GooseConfiguration;

const INDEX_PATH: &str = "/";
const REDIRECT_PATH: &str = "/redirect";
const REDIRECT2_PATH: &str = "/redirect2";
const REDIRECT3_PATH: &str = "/redirect3";
const ABOUT_PATH: &str = "/about.php";

const INDEX_KEY: usize = 0;
const REDIRECT_KEY: usize = 1;
const REDIRECT_KEY2: usize = 2;
const REDIRECT_KEY3: usize = 3;
const ABOUT_KEY: usize = 4;

const SERVER1_INDEX_KEY: usize = 0;
const SERVER1_ABOUT_KEY: usize = 1;
const SERVER1_REDIRECT_KEY: usize = 2;
const SERVER2_INDEX_KEY: usize = 3;
const SERVER2_ABOUT_KEY: usize = 4;

const EXPECT_WORKERS: usize = 2;
const USERS: usize = 4;

// Task function, load INDEX_PATH.
pub async fn get_index(user: &GooseUser) -> GooseTaskResult {
    let _goose = user.get(INDEX_PATH).await?;
    Ok(())
}

// Task function, load ABOUT PATH
pub async fn get_about(user: &GooseUser) -> GooseTaskResult {
    let _goose = user.get(ABOUT_PATH).await?;
    Ok(())
}

// Task function, load REDIRECT_PATH and follow redirects to ABOUT_PATH.
pub async fn get_redirect(user: &GooseUser) -> GooseTaskResult {
    let mut goose = user.get(REDIRECT_PATH).await?;

    if let Ok(r) = goose.response {
        match r.text().await {
            Ok(html) => {
                // Confirm that we followed redirects and loaded the about page.
                if !html.contains("about page") {
                    return user.set_failure(
                        "about page body wrong",
                        &mut goose.request,
                        None,
                        None,
                    );
                }
            }
            Err(e) => {
                return user.set_failure(
                    format!("unexpected error parsing about page: {}", e).as_str(),
                    &mut goose.request,
                    None,
                    None,
                );
            }
        }
    }
    Ok(())
}

// Task function, load REDIRECT_PATH and follow redirect to new domain.
pub async fn get_domain_redirect(user: &GooseUser) -> GooseTaskResult {
    let _goose = user.get(REDIRECT_PATH).await?;
    Ok(())
}

// Defines the different types of redirects being tested.
#[derive(Clone)]
enum TestType {
    // Chains many different redirects together.
    Chain,
    // Redirects between domains.
    Domain,
    // Permanently redirects between domains.
    Sticky,
}

// Sets up the endpoints used to test redirects.
fn setup_mock_server_endpoints<'a>(
    test_type: &TestType,
    server: &'a MockServer,
    server2: Option<&'a MockServer>,
) -> Vec<MockRef<'a>> {
    let mut endpoints: Vec<MockRef> = Vec::new();

    match test_type {
        TestType::Chain => {
            // First set up INDEX_PATH, store in vector at INDEX_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(INDEX_PATH)
                    .return_status(200)
                    .create_on(&server),
            );
            // Next set up REDIRECT_PATH, store in vector at REDIRECT_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(REDIRECT_PATH)
                    .return_status(301)
                    .return_header("Location", REDIRECT2_PATH)
                    .create_on(&server),
            );
            // Next set up REDIRECT2_PATH, store in vector at REDIRECT2_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(REDIRECT2_PATH)
                    .return_status(302)
                    .return_header("Location", REDIRECT3_PATH)
                    .create_on(&server),
            );
            // Next set up REDIRECT3_PATH, store in vector at REDIRECT3_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(REDIRECT3_PATH)
                    .return_status(303)
                    .return_header("Location", ABOUT_PATH)
                    .create_on(&server),
            );
            // Next set up ABOUT_PATH, store in vector at ABOUT_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(ABOUT_PATH)
                    .return_status(200)
                    .return_body("<HTML><BODY>about page</BODY></HTML>")
                    .create_on(&server),
            );
        }
        TestType::Domain | TestType::Sticky => {
            // First set up INDEX_PATH, store in vector at SERVER1_INDEX_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(INDEX_PATH)
                    .return_status(200)
                    .create_on(&server),
            );
            // Next set up ABOUT_PATH, store in vector at SERVER1_ABOUT_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(ABOUT_PATH)
                    .return_status(200)
                    .return_body("<HTML><BODY>about page</BODY></HTML>")
                    .create_on(&server),
            );
            // Next set up REDIRECT_PATH, store in vector at SERVER1_REDIRECT_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(REDIRECT_PATH)
                    .return_status(301)
                    .return_header("Location", &server2.unwrap().url(INDEX_PATH))
                    .create_on(&server),
            );
            // Next set up INDEX_PATH on server 2, store in vector at SERVER2_INDEX_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(INDEX_PATH)
                    .return_status(200)
                    .create_on(&server2.unwrap()),
            );
            // Next set up ABOUT_PATH on server 2, store in vector at SERVER2_ABOUT_KEY.
            endpoints.push(
                Mock::new()
                    .expect_method(GET)
                    .expect_path(ABOUT_PATH)
                    .return_status(200)
                    .create_on(&server2.unwrap()),
            );
        }
    }

    endpoints
}

// Build configuration for a load test.
fn common_build_configuration(
    server: &MockServer,
    sticky: bool,
    worker: Option<bool>,
    manager: Option<usize>,
) -> GooseConfiguration {
    if let Some(expect_workers) = manager {
        if sticky {
            common::build_configuration(
                &server,
                vec![
                    "--sticky-follow",
                    "--manager",
                    "--expect-workers",
                    &expect_workers.to_string(),
                    "--users",
                    &USERS.to_string(),
                    "--hatch-rate",
                    &USERS.to_string(),
                ],
            )
        } else {
            common::build_configuration(
                &server,
                vec![
                    "--manager",
                    "--expect-workers",
                    &expect_workers.to_string(),
                    "--users",
                    &USERS.to_string(),
                    "--hatch-rate",
                    &USERS.to_string(),
                ],
            )
        }
    } else if worker.is_some() {
        common::build_configuration(&server, vec!["--worker"])
    } else if sticky {
        common::build_configuration(
            &server,
            vec![
                "--sticky-follow",
                "--users",
                &USERS.to_string(),
                "--hatch-rate",
                &USERS.to_string(),
            ],
        )
    } else {
        common::build_configuration(
            &server,
            vec![
                "--users",
                &USERS.to_string(),
                "--hatch-rate",
                &USERS.to_string(),
            ],
        )
    }
}

// Common validation for the load tests in this file.
fn validate_redirect(test_type: &TestType, mock_endpoints: &[MockRef]) {
    match test_type {
        TestType::Chain => {
            // Confirm that all pages are loaded, even those not requested directly but
            // that are only loaded due to redirects.
            assert!(mock_endpoints[INDEX_KEY].times_called() > 0);
            assert!(mock_endpoints[REDIRECT_KEY].times_called() > 0);
            assert!(mock_endpoints[REDIRECT_KEY2].times_called() > 0);
            assert!(mock_endpoints[REDIRECT_KEY3].times_called() > 0);
            assert!(mock_endpoints[ABOUT_KEY].times_called() > 0);

            // Confirm the entire redirect chain is loaded the same number of times.
            assert!(
                mock_endpoints[REDIRECT_KEY].times_called()
                    == mock_endpoints[REDIRECT_KEY2].times_called()
            );
            assert!(
                mock_endpoints[REDIRECT_KEY].times_called()
                    == mock_endpoints[REDIRECT_KEY3].times_called()
            );
            assert!(
                mock_endpoints[REDIRECT_KEY].times_called()
                    == mock_endpoints[ABOUT_KEY].times_called()
            );
        }
        TestType::Domain => {
            // All pages on Server1 are loaded.
            assert!(mock_endpoints[SERVER1_INDEX_KEY].times_called() > 0);
            assert!(mock_endpoints[SERVER1_REDIRECT_KEY].times_called() > 0);
            assert!(mock_endpoints[SERVER1_ABOUT_KEY].times_called() > 0);

            // GooseUsers are redirected to Server2 correctly.
            assert!(mock_endpoints[SERVER2_INDEX_KEY].times_called() > 0);

            // GooseUsers do not stick to Server2 and load the other page.
            assert!(mock_endpoints[SERVER2_ABOUT_KEY].times_called() == 0);
        }
        TestType::Sticky => {
            // Each GooseUser loads the redirect on Server1 one time.
            assert!(mock_endpoints[SERVER1_REDIRECT_KEY].times_called() == USERS);

            // Redirected to Server2, no user load anything else on Server1.
            assert!(mock_endpoints[SERVER1_INDEX_KEY].times_called() == 0);
            assert!(mock_endpoints[SERVER1_ABOUT_KEY].times_called() == 0);

            // All GooseUsers go on to load pages on Server2.
            assert!(mock_endpoints[SERVER2_INDEX_KEY].times_called() > 0);
            assert!(mock_endpoints[SERVER2_ABOUT_KEY].times_called() > 0);
        }
    }
}

fn get_tasks(test_type: &TestType) -> GooseTaskSet {
    match test_type {
        TestType::Chain => {
            taskset!("LoadTest")
                // Load index directly.
                .register_task(task!(get_index))
                // Load redirect path, redirect to redirect2 path, redirect to
                // redirect3 path, redirect to about.
                .register_task(task!(get_redirect))
        }
        TestType::Domain | TestType::Sticky => {
            taskset!("LoadTest")
                // First load redirect, takes this request only to another domain.
                .register_task(task!(get_domain_redirect))
                // Load index.
                .register_task(task!(get_index))
                // Load about.
                .register_task(task!(get_about))
        }
    }
}

// Helper to run all standalone tests.
fn run_standalone_test(test_type: TestType) {
    // Start the mock servers.
    let server1 = MockServer::start();
    let server2 = MockServer::start();

    // Setup the endpoints needed for this test on the mock server.
    let mock_endpoints = setup_mock_server_endpoints(&test_type, &server1, Some(&server2));

    // Build appropriate configuration.
    let sticky = match test_type {
        TestType::Sticky => true,
        TestType::Chain | TestType::Domain => false,
    };
    let configuration = common_build_configuration(&server1, sticky, None, None);

    // Run the Goose Attack.
    common::run_load_test(
        common::build_load_test(configuration.clone(), &get_tasks(&test_type), None, None),
        None,
    );

    // Confirm that the load test was actually redirected.
    validate_redirect(&test_type, &mock_endpoints);
}

// Helper to run all standalone tests.
fn run_gaggle_test(test_type: TestType) {
    // Start the mock servers.
    let server1 = MockServer::start();
    let server2 = MockServer::start();

    // Setup the endpoints needed for this test on the mock server.
    let mock_endpoints = setup_mock_server_endpoints(&test_type, &server1, Some(&server2));

    // Build appropriate Worker configuration.
    let sticky = match test_type {
        TestType::Sticky => true,
        TestType::Chain | TestType::Domain => false,
    };
    let worker_configuration = common_build_configuration(&server1, sticky, Some(true), None);

    // Build the load test for the Workers.
    let worker_goose_attack =
        common::build_load_test(worker_configuration, &get_tasks(&test_type), None, None);

    // Workers launched in own threads, store thread handles.
    let worker_handles = common::launch_gaggle_workers(worker_goose_attack, EXPECT_WORKERS);

    // Build Manager configuration.
    let manager_configuration =
        common_build_configuration(&server1, sticky, None, Some(EXPECT_WORKERS));

    // Build the load test for the Workers.
    let manager_goose_attack =
        common::build_load_test(manager_configuration, &get_tasks(&test_type), None, None);

    // Run the Goose Attack.
    common::run_load_test(manager_goose_attack, Some(worker_handles));

    // Confirm that the load test was actually redirected.
    validate_redirect(&test_type, &mock_endpoints);
}

#[test]
/// Simulate a load test which includes a page with a redirect chain, confirms
/// all redirects are correctly followed.
fn test_redirect() {
    run_standalone_test(TestType::Chain);
}

#[test]
// Only run gaggle tests if the feature is compiled into the codebase.
#[cfg_attr(not(feature = "gaggle"), ignore)]
// Gaggle tests have to be running serially instead of in parallel.
#[serial]
/// Simulate a distributed load test which includes a page with a redirect chain,
/// confirms all redirects are correctly followed.
fn test_redirect_gaggle() {
    run_gaggle_test(TestType::Chain);
}

#[test]
/// Simulate a load test which includes a page with a redirect to another domain
/// (which in this case is a second mock server running on a different path).
/// Confirm all redirects are correctly followed.
fn test_domain_redirect() {
    run_standalone_test(TestType::Domain);
}

#[test]
// Only run gaggle tests if the feature is compiled into the codebase.
#[cfg_attr(not(feature = "gaggle"), ignore)]
// Gaggle tests have to be running serially instead of in parallel.
#[serial]
/// Simulate a distributed load test which includes a page with a redirect to
/// another domain (which in this case is a second mock server running on a
/// different path). Confirm all redirects are correctly followed.
fn test_domain_redirect_gaggle() {
    run_gaggle_test(TestType::Domain);
}

#[test]
/// Simulate a load test which permanently follows a redirect due to the
/// --sticky-follow run-time option.
fn test_sticky_domain_redirect() {
    run_standalone_test(TestType::Sticky);
}

#[test]
// Only run gaggle tests if the feature is compiled into the codebase.
#[cfg_attr(not(feature = "gaggle"), ignore)]
// Gaggle tests have to be running serially instead of in parallel.
#[serial]
/// Simulate a distributed load test which permanently follows a redirect
/// due to the --sticky-follow run-time option.
fn test_sticky_domain_redirect_gaggle() {
    run_gaggle_test(TestType::Sticky);
}
