use std::{process, str::FromStr};

use inquire::{validator::Validation, Confirm, Select, Text};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};
use twilly::{account::Status, Client};
use twilly_cli::{
    get_action_choice_from_user, get_filter_choice_from_user, prompt_user, prompt_user_selection,
    ActionChoice, FilterChoice,
};

#[derive(Debug, Clone, Display, EnumIter, EnumString)]
pub enum Action {
    #[strum(to_string = "Get sub-account")]
    GetSubAccount,
    #[strum(to_string = "List sub-accounts")]
    ListSubAccounts,
    #[strum(to_string = "Create sub-account")]
    CreateSubAccount,
    Back,
    Exit,
}

pub async fn choose_account_action(twilio: &Client) {
    let options: Vec<Action> = Action::iter().collect();

    loop {
        let action_selection_prompt = Select::new("Select an action:", options.clone());

        if let Some(action) = prompt_user_selection(action_selection_prompt) {
            match action {
                Action::GetSubAccount => {
                    let account_sid_prompt = Text::new("Please provide an account SID:")
                        .with_placeholder("AC...")
                        .with_help_message(
                            "This will only find sub-accounts on your existing profile.",
                        )
                        .with_validator(|val: &str| match val.starts_with("AC") {
                            true => Ok(Validation::Valid),
                            false => {
                                Ok(Validation::Invalid("Account SID must start with AC".into()))
                            }
                        })
                        .with_validator(|val: &str| match val.len() {
                            34 => Ok(Validation::Valid),
                            _ => Ok(Validation::Invalid(
                                "Your SID should be 34 characters in length".into(),
                            )),
                        });

                    if let Some(account_sid) = prompt_user(account_sid_prompt) {
                        let account = twilio
                            .accounts()
                            .get(Some(&account_sid))
                            .await
                            .unwrap_or_else(|error| panic!("{}", error));
                        println!("{:#?}", account);
                        println!();
                    }
                }
                Action::CreateSubAccount => {
                    let friendly_name_prompt =
                        Text::new("Enter a friendly name (empty for default):");

                    if let Some(friendly_name) = prompt_user(friendly_name_prompt) {
                        println!("Creating sub-account...");
                        let account = twilio
                            .accounts()
                            .create(Some(&friendly_name))
                            .await
                            .unwrap_or_else(|error| panic!("{}", error));
                        println!(
                            "Sub-account created: {} ({})",
                            account.friendly_name, account.sid
                        );
                    }
                }
                Action::ListSubAccounts => {
                    let friendly_name_prompt =
                        Text::new("Search by friendly name? (empty for none):");

                    if let Some(friendly_name) = prompt_user(friendly_name_prompt) {
                        if let Some(filter_choice) = get_filter_choice_from_user(
                            Status::iter().map(|status| status.to_string()).collect(),
                            "Filter by status: ",
                        ) {
                            let status = match filter_choice {
                                FilterChoice::Any => None,
                                FilterChoice::Other(choice) => Some(
                                    Status::from_str(&choice)
                                        .expect("Unable to determine account status"),
                                ),
                            };

                            println!("Retrieving accounts...");
                            let mut accounts = twilio
                                .accounts()
                                .list(Some(&friendly_name), status.as_ref())
                                .await
                                .unwrap_or_else(|error| panic!("{}", error));

                            // The action we can perform on the account we are using are limited.
                            // Remove it from the list.
                            accounts.retain(|ac| ac.sid != twilio.config.account_sid);

                            if accounts.is_empty() {
                                println!("No sub-accounts found.");
                                break;
                            }

                            println!("Found {} sub-accounts.", accounts.len());

                            // Stores the index of the account the user is currently interacting
                            // with. For the first loop this is certainly `None`.
                            let mut selected_account_index: Option<usize> = None;
                            loop {
                                // If we know the index (a.k.a it hasn't been cleared by some other operation)
                                // then use this account otherwise let the user choice.
                                let selected_account = if let Some(index) = selected_account_index {
                                    &mut accounts[index]
                                } else if let Some(action_choice) = get_action_choice_from_user(
                                    accounts
                                        .iter()
                                        .map(|ac| {
                                            format!(
                                                "({}) {} - {}",
                                                ac.sid, ac.friendly_name, ac.status
                                            )
                                        })
                                        .collect::<Vec<String>>(),
                                    "Accounts: ",
                                ) {
                                    match action_choice {
                                        ActionChoice::Back => {
                                            break;
                                        }
                                        ActionChoice::Exit => process::exit(0),
                                        ActionChoice::Other(choice) => {
                                            let account_position = accounts
                                                .iter()
                                                .position(
                                                    |account| account.sid == choice[1..35]
                                                )
                                                .expect(
                                                    "Could not find sub-account in existing sub-account list"
                                                );

                                            selected_account_index = Some(account_position);
                                            &mut accounts[account_position]
                                        }
                                    }
                                } else {
                                    break;
                                };

                                match selected_account.status.as_str() {
                                    "active" => {
                                        if let Some(account_action) = get_action_choice_from_user(
                                            vec![
                                                "Change name".into(),
                                                "Suspend".into(),
                                                "Close".into(),
                                            ],
                                            "Select an action: ",
                                        ) {
                                            match account_action {
                                                ActionChoice::Back => {
                                                    break;
                                                }
                                                ActionChoice::Exit => process::exit(0),
                                                ActionChoice::Other(choice) => {
                                                    match choice.as_str() {
                                                        "Change name" => {
                                                            change_account_name(
                                                                twilio,
                                                                &selected_account.sid,
                                                            )
                                                            .await;
                                                            accounts[selected_account_index
                                                                .expect(
                                                                "Selected sub-account is unknown",
                                                            )]
                                                            .friendly_name = friendly_name.clone();
                                                        }
                                                        "Suspend" => {
                                                            suspend_account(
                                                                twilio,
                                                                &selected_account.sid,
                                                            )
                                                            .await;
                                                            accounts[selected_account_index
                                                                .expect(
                                                                "Selected sub-account is unknown",
                                                            )]
                                                            .status = Status::Suspended;
                                                        }
                                                        "Close" => {
                                                            close_account(
                                                                twilio,
                                                                &selected_account.sid,
                                                            )
                                                            .await;
                                                            accounts[selected_account_index
                                                                .expect(
                                                                "Selected sub-account is unknown",
                                                            )]
                                                            .status = Status::Closed;
                                                        }
                                                        _ => {
                                                            println!("Unknown action '{}'", choice);
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                    "suspended" => {
                                        if let Some(account_action) = get_action_choice_from_user(
                                            vec!["Change name".into(), "Activate".into()],
                                            "Select an action: ",
                                        ) {
                                            match account_action {
                                                ActionChoice::Back => {
                                                    break;
                                                }
                                                ActionChoice::Exit => process::exit(0),
                                                ActionChoice::Other(choice) => {
                                                    match choice.as_str() {
                                                        "Change name" => {
                                                            change_account_name(
                                                                twilio,
                                                                &selected_account.sid,
                                                            )
                                                            .await;
                                                            accounts[selected_account_index
                                                                .expect(
                                                                    "Selected account is unknown",
                                                                )]
                                                            .friendly_name = friendly_name.clone();
                                                        }
                                                        "Activate" => {
                                                            activate_account(
                                                                twilio,
                                                                &selected_account.sid,
                                                            )
                                                            .await;
                                                            accounts[selected_account_index
                                                                .expect(
                                                                "Selected sub-account is unknown",
                                                            )]
                                                            .status = Status::Active;
                                                        }

                                                        _ => {
                                                            println!("Unknown action '{}'", choice);
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            break;
                                        };
                                    }
                                    "closed" => {
                                        println!(
                                            "{} is a closed sub-account and can no longer be used.",
                                            selected_account.sid
                                        );
                                    }
                                    _ => {
                                        println!(
                                            "Unknown account type '{}'",
                                            selected_account.status
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Action::Back => {
                    break;
                }
                Action::Exit => process::exit(0),
            }
        } else {
            break;
        }
    }
}

#[allow(clippy::println_empty_string)]
async fn change_account_name(twilio: &Client, account_sid: &str) {
    let friendly_name_prompt =
        Text::new("Provide a name:").with_validator(|val: &str| match !val.is_empty() {
            true => Ok(Validation::Valid),
            false => Ok(Validation::Invalid("Enter at least one character".into())),
        });

    if let Some(friendly_name) = prompt_user(friendly_name_prompt) {
        println!("Updating account...");
        let updated_account = twilio
            .accounts()
            .update(account_sid, Some(&friendly_name), None)
            .await
            .unwrap_or_else(|error| panic!("{}", error));

        println!("{:#?}", updated_account);
        println!("");
    }
}

#[allow(clippy::println_empty_string)]
async fn activate_account(twilio: &Client, account_sid: &str) {
    let confirmation_prompt = Confirm::new("Are you sure you wish to activate this sub-account?")
        .with_placeholder("N")
        .with_default(false);

    if let Some(confirmation) = prompt_user(confirmation_prompt) {
        if confirmation {
            println!("Activating sub-account...");
            twilio
                .accounts()
                .update(account_sid, None, Some(&Status::Suspended))
                .await
                .unwrap_or_else(|error| panic!("{}", error));

            println!("Aaccount activated.");
            return;
        }
    }

    println!("Operation canceled. No changes were made.");
}

async fn suspend_account(twilio: &Client, account_sid: &str) {
    let confirmation_prompt = Confirm::new(
        "Are you sure you wish to suspend this sub-account? Any activity will be disabled until the account is re-activated."
    )
        .with_placeholder("N")
        .with_default(false);

    if let Some(confirmation) = prompt_user(confirmation_prompt) {
        if confirmation {
            println!("Suspending sub-account...");
            let res = twilio
                .accounts()
                .update(account_sid, None, Some(&Status::Suspended))
                .await
                .unwrap_or_else(|error| panic!("{}", error));

            println!("{}", res);
            println!("Account suspended.");
            return;
        }
    }

    println!("Operation canceled. No changes were made.");
}

async fn close_account(twilio: &Client, account_sid: &str) {
    let confirmation_prompt = Confirm::new(
        "Are you sure you wish to Close this sub-account? Activity will be disabled and this action cannot be reversed."
    )
        .with_placeholder("N")
        .with_default(false);

    if let Some(confirmation) = prompt_user(confirmation_prompt) {
        if confirmation {
            println!("Closing account...");
            twilio
                .accounts()
                .update(account_sid, None, Some(&Status::Suspended))
                .await
                .unwrap_or_else(|error| panic!("{}", error));

            println!(
                "Account closed. This account will still be visible in the console for 30 days."
            );
            return;
        }
    }

    println!("Operation canceled. No changes were made.");
}
