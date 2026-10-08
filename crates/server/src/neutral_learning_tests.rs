//! Trusted-auth protocol fixtures only; this does not open Hargow's production gate.
use crate::product::ProductId;
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::{Value, json};
use tower::ServiceExt;
fn learning_app(db: &DatabaseConnection, product: ProductId, user: i64) -> Router {
    let identity=serde_json::from_value(json!({"product":product.as_str(),"account":{"id":user.to_string(),"email":format!("protocol-{user}@example.test"),"displayName":"Protocol","role":"learner","version":1},"membership":{"role":"learner","version":1}})).unwrap();
    crate::learning_store::routes()
        .with_state(crate::learning_store::LearningStore::for_product(
            db.clone(),
            product,
        ))
        .layer(axum::Extension(crate::learning_identity::LearningAuth {
            identity,
        }))
}
pub(crate) async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("x-product", "brioche");
    let body = if let Some(body) = body {
        request = request.header("content-type", "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
pub(crate) fn start_request() -> Value {
    json!({"lessonId":"neutral-publish","schemaVersion":"2.0","idempotencyKey":"neutral-start-0001"})
}
pub(crate) async fn exercise(db: &DatabaseConnection, identity_schema: &str) -> (Router, String) {
    db.execute_unprepared(&format!("INSERT INTO {identity_schema}.users(id,email,password_hash,display_name) VALUES(101,'protocol-101@example.test','synthetic','Protocol'),(102,'protocol-102@example.test','synthetic','Other'); INSERT INTO {identity_schema}.product_memberships(product_id,user_id,role) VALUES('hargow',101,'learner'),('hargow',102,'learner'),('brioche',101,'learner')")).await.unwrap();
    let app = learning_app(db, ProductId::Hargow, 101);
    let foreign = learning_app(db, ProductId::Brioche, 101);
    let other = learning_app(db, ProductId::Hargow, 102);
    assert_eq!(
        request(
            &foreign,
            "POST",
            "/api/v2/learning-sessions",
            Some(start_request())
        )
        .await
        .0,
        404
    );
    let (status, mut session) = request(
        &app,
        "POST",
        "/api/v2/learning-sessions",
        Some(start_request()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(session["lesson"]["targetLanguage"], "yue-Hant-HK");
    assert!(session["lesson"].get("serverOnly").is_none());
    serde_json::from_value::<brioche_course_contract::neutral::NeutralLearningSession>(
        session.clone(),
    )
    .unwrap();
    let id = session["progress"]["id"].as_str().unwrap().to_owned();
    let path = format!("/api/v2/learning-sessions/{id}");
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/v2/learning-sessions",
            Some(start_request())
        )
        .await,
        (200, session.clone())
    );
    for stranger in [&foreign, &other] {
        assert_eq!(request(stranger, "GET", &path, None).await.0, 404);
    }
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/api/v1/learning-sessions/{id}"),
            None
        )
        .await
        .0,
        409
    );
    assert_eq!(request(&app,"POST","/api/v1/learning-sessions",Some(json!({"lessonId":"neutral-publish","schemaVersion":"1.0","idempotencyKey":"neutral-legacy-0001"}))).await.0,409);
    let write = |v: Value, key: &str| json!({"version":v,"idempotencyKey":key});
    let v = session["progress"]["version"].clone();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{path}/complete"),
            Some(write(v.clone(), "neutral-early-0001"))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{path}/hints/text"),
            Some(write(v.clone(), "neutral-early-hint"))
        )
        .await
        .0,
        409
    );
    let (_, state) = request(
        &app,
        "PUT",
        &format!("{path}/steps/step-read"),
        Some(write(v, "neutral-read-0001")),
    )
    .await;
    let mut state = state;
    let hint_request = write(state["version"].clone(), "neutral-hint-0001");
    let (mut status, hint) = request(
        &app,
        "POST",
        &format!("{path}/hints/text"),
        Some(hint_request.clone()),
    )
    .await;
    assert_eq!(status, 200);
    assert!(!hint["hintZh"].as_str().unwrap().is_empty());
    state = hint["progress"].clone();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{path}/hints/text"),
            Some(hint_request)
        )
        .await,
        (200, hint)
    );
    let invalid = json!({"version":state["version"],"idempotencyKey":"neutral-invalid-01","exerciseId":"choice","answer":{"kind":"choice","optionId":"unknown"}});
    assert_eq!(
        request(&app, "POST", &format!("{path}/attempts"), Some(invalid))
            .await
            .0,
        400
    );
    assert_eq!(request(&app, "GET", &path, None).await.1["progress"], state);
    let accepted: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/neutral-cantonese.lesson.json"
    ))
    .unwrap();
    for (exercise, answer, key) in [
        (
            "choice",
            json!({"kind":"choice","optionId":"greeting"}),
            "neutral-choice-001",
        ),
        (
            "text",
            json!({"kind":"text","text":accepted["serverOnly"]["grading"]["text"]["accepted"][0]}),
            "neutral-text-00001",
        ),
        (
            "order",
            json!({"kind":"order","tokenIds":["second","bye","first"]}),
            "neutral-order-0001",
        ),
    ] {
        let attempt = json!({"version":state["version"],"idempotencyKey":key,"exerciseId":exercise,"answer":answer});
        let (status, result) = request(
            &app,
            "POST",
            &format!("{path}/attempts"),
            Some(attempt.clone()),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(result["result"]["correct"], true);
        state = result["progress"].clone();
        assert_eq!(
            request(&app, "POST", &format!("{path}/attempts"), Some(attempt)).await,
            (200, result)
        );
    }
    assert!(
        state["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["exerciseId"] == "text" && a["hintUsed"] == true)
    );
    for step in ["step-practice", "step-recap"] {
        (status, state) = request(
            &app,
            "PUT",
            &format!("{path}/steps/{step}"),
            Some(write(
                state["version"].clone(),
                &format!("neutral-{step}-01"),
            )),
        )
        .await;
        assert_eq!(status, 200);
    }
    let complete = write(state["version"].clone(), "neutral-complete-1");
    (status, state) = request(
        &app,
        "POST",
        &format!("{path}/complete"),
        Some(complete.clone()),
    )
    .await;
    assert_eq!(status, 200);
    assert!(state["completedAt"].is_string());
    assert_eq!(
        request(&app, "POST", &format!("{path}/complete"), Some(complete)).await,
        (200, state.clone())
    );
    (status, session) = request(&app, "GET", &path, None).await;
    assert_eq!(status, 200);
    assert_eq!(session["progress"], state);
    let row = crate::learning::one(
        db,
        "SELECT schema_version FROM learning_sessions WHERE id=$1 AND product_id='hargow'",
        vec![id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        crate::learning::field::<String>(&row, "schema_version").unwrap(),
        "2.0"
    );
    let row = crate::learning::one(
        db,
        "SELECT snapshot FROM review_cards WHERE user_id=101 AND product_id='hargow'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    let snapshot: Value = crate::learning::field(&row, "snapshot").unwrap();
    assert_eq!(
        snapshot["lemma"],
        accepted["knowledge"]["vocabulary"][0]["lemma"]
    );
    let (status, overview) = request(&app, "GET", "/api/v2/me/learning", None).await;
    assert_eq!(status, 200);
    assert_eq!(overview["items"][0]["sessionId"], id);
    assert_eq!(overview["items"][0]["title"], session["lesson"]["title"]);
    let (status, dashboard) = request(&app, "GET", "/api/v2/me/dashboard", None).await;
    assert_eq!(status, 200);
    assert_eq!(dashboard["completedLessons"], 1);
    assert_eq!(dashboard["dueReviews"], 1);
    assert_eq!(
        dashboard["courseStates"][0]["title"],
        session["lesson"]["title"]
    );
    assert_eq!(
        dashboard["recommendedLesson"]["targetLanguage"],
        "yue-Hant-HK"
    );
    assert_eq!(dashboard["allAvailableCompleted"], true);
    for stranger in [&foreign, &other] {
        let (status, overview) = request(stranger, "GET", "/api/v2/me/learning", None).await;
        assert_eq!(status, 200);
        assert!(overview["items"].as_array().unwrap().is_empty());
    }
    let (status, queue) = request(&app, "GET", "/api/v2/me/reviews", None).await;
    assert_eq!(status, 200);
    assert_eq!(queue["dueCount"], 1);
    let card = queue["items"][0].clone();
    let card_id = card["id"].as_str().unwrap();
    let card_path = format!("/api/v2/me/reviews/{card_id}");
    assert_eq!(card["vocabulary"]["lemma"], snapshot["lemma"]);
    for stranger in [&foreign, &other] {
        assert_eq!(request(stranger, "GET", &card_path, None).await.0, 404);
    }
    assert_eq!(
        request(&app, "GET", &format!("/api/v1/me/reviews/{card_id}"), None)
            .await
            .0,
        409
    );
    let enroll = json!({"knowledgeId":"expr-greeting","sourceLessonId":"neutral-publish","sourceRevision":1,"idempotencyKey":"neutral-enroll-001"});
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/v2/me/review-enrollments",
            Some(enroll.clone())
        )
        .await,
        (200, card.clone())
    );
    assert_eq!(
        request(&app, "POST", "/api/v2/me/review-enrollments", Some(enroll)).await,
        (200, card.clone())
    );
    let saved = json!({"sourceLessonId":"neutral-publish","sourceRevision":1,"saved":true,"version":0,"idempotencyKey":"neutral-saved-001"});
    let saved_path = "/api/v2/me/saved-items/expr-greeting";
    let (status, saved_item) = request(&app, "PUT", saved_path, Some(saved.clone())).await;
    assert_eq!(status, 200);
    assert_eq!(saved_item["vocabulary"], card["vocabulary"]);
    assert_eq!(
        request(&app, "PUT", saved_path, Some(saved)).await,
        (200, saved_item.clone())
    );
    assert_eq!(
        request(&app, "GET", saved_path, None).await,
        (200, saved_item.clone())
    );
    let (status, saved_page) = request(&app, "GET", "/api/v2/me/saved-items", None).await;
    assert_eq!(status, 200);
    assert_eq!(saved_page["items"][0], saved_item);
    for stranger in [&foreign, &other] {
        assert_eq!(request(stranger, "GET", saved_path, None).await.0, 404);
    }
    let mut preferences = json!({"cardVersion":card["version"],"suspended":true,"idempotencyKey":"neutral-suspend-01"});
    let (status, suspended) = request(
        &app,
        "PUT",
        &format!("{card_path}/preferences"),
        Some(preferences.clone()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(suspended["suspended"], true);
    assert_eq!(
        request(
            &app,
            "PUT",
            &format!("{card_path}/preferences"),
            Some(preferences.clone())
        )
        .await,
        (200, suspended.clone())
    );
    assert_eq!(
        request(&app, "GET", "/api/v2/me/reviews", None).await.1["dueCount"],
        0
    );
    preferences["cardVersion"] = suspended["version"].clone();
    preferences["suspended"] = json!(false);
    preferences["idempotencyKey"] = json!("neutral-resume-001");
    let (status, resumed) = request(
        &app,
        "PUT",
        &format!("{card_path}/preferences"),
        Some(preferences),
    )
    .await;
    assert_eq!(status, 200);
    let attempt = json!({"cardVersion":resumed["version"],"idempotencyKey":"neutral-review-001","rating":"remembered"});
    let (status, reviewed) = request(
        &app,
        "POST",
        &format!("{card_path}/attempts"),
        Some(attempt.clone()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(reviewed["card"]["vocabulary"], card["vocabulary"]);
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{card_path}/attempts"),
            Some(attempt)
        )
        .await,
        (200, reviewed.clone())
    );
    let (status, history) = request(&app, "GET", "/api/v2/me/review-history", None).await;
    assert_eq!(status, 200);
    assert_eq!(history["items"][0]["vocabulary"], card["vocabulary"]);
    let (status, cards) = request(&app, "GET", "/api/v2/me/review-cards", None).await;
    assert_eq!(status, 200);
    assert_eq!(cards["items"][0], reviewed["card"]);
    assert_eq!(
        request(&app, "GET", "/api/v2/me/reviews?product=brioche", None)
            .await
            .0,
        400
    );
    let row = crate::learning::one(
        db,
        "SELECT count(*)::bigint AS count FROM learning_sessions WHERE product_id='brioche'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(crate::learning::field::<i64>(&row, "count").unwrap(), 0);
    drop(foreign);
    drop(other);
    (app, id)
}
