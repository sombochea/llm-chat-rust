use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::{convert::Infallible, io::Write, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ChatRequest {
    prompt: String,
    model_path: String,
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    response: String,
}

fn infer(model_path: String, prompt: String) -> Result<String, Box<dyn std::error::Error>> {
    let tokenizer_source = llm::TokenizerSource::Embedded;
    let model_architecture = llm::ModelArchitecture::Llama;
    let model_path = PathBuf::from(model_path);
    let prompt = prompt.to_string();
    let now = std::time::Instant::now();
    let model = llm::load_dynamic(
        Some(model_architecture),
        &model_path,
        tokenizer_source,
        Default::default(),
        llm::load_progress_callback_stdout,
    )?;

    println!(
        "Model fully loaded! Elapsed: {}ms",
        now.elapsed().as_millis()
    );

    let mut session = model.start_session(Default::default());
    let mut generated_tokens = String::new(); // Accumulate generated tokens here

    let res = session.infer::<Infallible>(
        model.as_ref(),
        &mut rand::thread_rng(),
        &llm::InferenceRequest {
            prompt: (&prompt).into(),
            parameters: &llm::InferenceParameters::default(),
            play_back_previous_tokens: false,
            maximum_token_count: Some(140),
        },
        // OutputRequest
        &mut Default::default(),
        |r| match r {
            llm::InferenceResponse::PromptToken(t) | llm::InferenceResponse::InferredToken(t) => {
                print!("{t}");
                std::io::stdout().flush().unwrap();
                // Accumulate generated tokens
                generated_tokens.push_str(&t);
                Ok(llm::InferenceFeedback::Continue)
            }
            _ => Ok(llm::InferenceFeedback::Continue),
        },
    );

    // Return the accumulated generated tokens
    match res {
        Ok(_) => Ok(generated_tokens),
        Err(err) => Err(Box::new(err)),
    }
}

async fn chat_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
    let chat_request: Result<ChatRequest, _> = serde_json::from_slice(&body_bytes);

    match chat_request {
        Ok(chat_request) => {
            // Call the `infer` function with the received prompt
            match infer(chat_request.model_path, chat_request.prompt) {
                Ok(inference_result) => {
                    // Prepare the response message
                    let response_message = format!("Inference result: {}", inference_result);
                    let chat_response = ChatResponse {
                        response: response_message,
                    };
                    // Serialize the response and send it back
                    let response =
                        Response::new(Body::from(serde_json::to_string(&chat_response).unwrap()));
                    Ok(response)
                }
                Err(err) => {
                    eprintln!("Error in inference: {:?}", err);
                    // Return a 500 Internal Server Error response
                    Ok(Response::builder().status(500).body(Body::empty()).unwrap())
                }
            }
        }
        Err(_) => {
            // Return a 400 Bad Request response for JSON deserialization failure
            Ok(Response::builder().status(400).body(Body::empty()).unwrap())
        }
    }
}

async fn router(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.uri().path(), req.method()) {
        ("/api/chat", &hyper::Method::POST) => chat_handler(req).await,
        _ => not_found(),
    }
}

fn not_found() -> Result<Response<Body>, Infallible> {
    // Return a 404 Not Found response
    Ok(Response::builder().status(404).body(Body::empty()).unwrap())
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("PORT must be a number");

    println!("Server listening on port {}", port);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(router)) });
    let server = Server::bind(&addr).serve(make_svc);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
