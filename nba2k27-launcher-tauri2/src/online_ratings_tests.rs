use super::*;
#[test]
fn exact_player_urls_and_roster_identity() {
    for u in [
        "https://2kdb.net/player/123",
        "https://2kauction.com/mynba/2k27/players/example-player?roster=current&version=42",
        "https://2kauction.com/mynba/players/example?game=2k27",
    ] {
        assert!(Address::parse(u).is_ok(), "{u}");
    }
    for u in [
        "http://2kdb.net/player/1",
        "https://2kdb.net.evil/player/1",
        "https://2kdb.net:443/player/1",
        "https://user@2kdb.net/player/1",
        "https://2kdb.net/player/0",
        "https://2kdb.net/player/1?x=1",
        "https://2kdb.net/player/1#fragment",
        "https://2kauction.com/mynba/2k26/players/player?game=2k27",
        "https://2kauction.com/mynba/players/player?game=2k27&game=2k27",
        "https://2kauction.com/mynba/players/player?token=secret",
        "file:///C:/private",
    ] {
        assert!(Address::parse(u).is_err(), "{u}");
    }
}
#[test]
fn resources_navigation_and_source_are_separate() {
    let db = Address::parse("https://2kdb.net/player/1").unwrap();
    for u in [
        "https://2kdb.net/api/player",
        "https://img.2kdb.net/image.png",
        "https://challenges.cloudflare.com/challenge",
        "data:image/png;base64,a",
        "blob:https://2kdb.net/id",
    ] {
        assert!(db.resource(u), "{u}");
    }
    for u in [
        "https://evil.test/",
        "https://2kdb.net.evil/",
        "http://localhost:3000/",
        "ws://2kdb.net/",
        "wss://2kdb.net/",
        "tauri://localhost/",
        "https://2kdb.net:444/",
        "file:///C:/game",
    ] {
        assert!(!db.resource(u), "{u}");
    }
    assert!(!db.navigation("https://challenges.cloudflare.com/"));
    assert!(db.navigation("https://2kdb.net/player/2"));
    assert!(!db.is_source("https://2kdb.net/player/2"));
    let auction =
        Address::parse("https://2kauction.com/mynba/players/player?game=2k27&roster=current")
            .unwrap();
    assert!(!auction.resource("https://img.2kdb.net/"));
    assert!(
        !auction.is_source("https://2kauction.com/mynba/players/player?game=2k26&roster=current")
    );
}
#[test]
fn destination_is_bounded_and_not_a_path_or_game_capability() {
    let v = json!({"recordIndex":0,"firstName":"A","lastName":"B","cyberfaceId":42});
    assert!(destination(&v).is_ok());
    let mut x = v.clone();
    x["path"] = json!("C:/private");
    assert!(destination(&x).is_err());
    let mut x = v.clone();
    x["recordIndex"] = json!(-1);
    assert!(destination(&x).is_err());
    let mut x = v.clone();
    x["firstName"] = json!("x".repeat(101));
    assert!(destination(&x).is_err());
}
#[test]
fn service_rejects_unowned_request_before_any_window_or_profile() {
    let service = Service::new();
    let root = Path::new(r"C:\must-not-create");
    assert!(service.request(root,0,"untrusted",&json!({"action":"start","url":"https://2kdb.net/player/1","destination":{"recordIndex":0,"firstName":"A","lastName":"B","cyberfaceId":1}})).is_err());
    service.cancel();
    assert!(
        service
            .request(
                root,
                0,
                "01234567-1234-1234-1234-0123456789ab",
                &json!({"action":"start"})
            )
            .is_err()
    );
}
