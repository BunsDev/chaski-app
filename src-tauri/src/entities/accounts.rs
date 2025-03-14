use crate::db::establish_connection;
use crate::models::Account;
use crate::models::NewAccount;
use crate::schema::accounts::dsl::*;
use crate::schema::{articles, feeds, filters};
use diesel::prelude::*;
use tokio::time::{sleep, Duration};

pub fn index(app_handle: tauri::AppHandle) -> Vec<Account> {
    accounts
        .select(Account::as_select())
        .load(&mut establish_connection(&app_handle))
        .expect("Error loading configurations")
}

pub fn create(app_handle: tauri::AppHandle, account: NewAccount) -> Account {
    diesel::insert_into(accounts)
        .values(&account)
        .returning(Account::as_returning())
        .get_result(&mut establish_connection(&app_handle))
        .expect("Error creating new account")
}

pub fn show(account_id: i32, app_handle: tauri::AppHandle) -> Option<Account> {
    let conn = &mut establish_connection(&app_handle);

    accounts
        .find(account_id)
        .select(Account::as_select())
        .first(conn)
        .optional()
        .expect("Error finding account")
}

pub fn destroy(account_id_eq: i32, app_handle: tauri::AppHandle) -> Result<(), String> {
    let conn = &mut establish_connection(&app_handle);

    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let feed_ids: Vec<i32> = feeds::table
            .select(feeds::id)
            .filter(feeds::account_id.eq(account_id_eq))
            .load(conn)?;

        diesel::delete(articles::table.filter(articles::feed_id.eq_any(&feed_ids)))
            .execute(conn)?;

        diesel::delete(filters::table.filter(filters::feed_id.eq_any(&feed_ids))).execute(conn)?;

        diesel::delete(feeds::table.filter(feeds::account_id.eq(account_id_eq))).execute(conn)?;

        diesel::delete(accounts.find(account_id_eq)).execute(conn)?;

        Ok(())
    })
    .map_err(|e| {
        log::error!("Error deleting account {}: {}", account_id_eq, e);
        format!("Failed to delete account: {}", e)
    })
}

pub fn spawn_greaderapi_accounts_sync_loop(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let _ = greaderapi_accounts_sync_loop(app_handle).await;
    });
}

async fn greaderapi_accounts_sync_loop(app_handle: tauri::AppHandle) {
    loop {
        let conn = &mut establish_connection(&app_handle);

        let greader_accounts: Vec<Account> = accounts
            .filter(kind.eq("greaderapi"))
            .load(conn)
            .expect("Error loading GReader accounts");

        for account in greader_accounts {
            log::info!(target: "chaski:sync", "Starting sync for account: {} (ID: {})", account.name, account.id);

            let cloned_app_handle = app_handle.clone();
            match crate::feeds::full_sync_greaderapi_account_feeds(&account, cloned_app_handle)
                .await
            {
                Ok(_) => {
                    log::info!(target: "chaski:sync", "Successfully synced account: {} (ID: {})", account.name, account.id);
                }
                Err(e) => {
                    log::error!(target: "chaski:sync", "Error syncing account {} (ID: {}): {}", account.name, account.id, e);
                }
            }
        }

        sleep(Duration::from_secs(60 * 60)).await;
    }
}
