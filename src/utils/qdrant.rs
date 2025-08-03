use qdrant_client::qdrant::{CreateCollection, Distance, PointId, PointStruct, UpsertPoints, VectorParams, Vectors, VectorsConfig};
use qdrant_client::Qdrant;
use uuid::Uuid;
use crate::app::error::AppError;
use crate::app::result::AppResult;
use serde_json::json;
use qdrant_client::qdrant::{SearchPoints, Filter, Condition};


use qdrant_client::qdrant::{
    Datatype, HnswConfigDiff
};

pub async fn ensure_collection(client: &Qdrant) -> AppResult<()> {
    let exists = client.collection_exists("chat_memory")
        .await
        .map_err(|e| AppError::QdrantError(e.to_string()))?;

    if !exists {
        client.create_collection(CreateCollection {
            collection_name: "chat_memory".to_string(),
            vectors_config: Some(VectorsConfig {
                config: Some(qdrant_client::qdrant::vectors_config::Config::Params(
                    VectorParams {
                        size: 1536, // [ขนาด embedding model]
                        distance: Distance::Cosine.into(),
                        datatype: Some(Datatype::Float32 as i32),
                        hnsw_config: Some(HnswConfigDiff::default()),
                        multivector_config: None,
                        on_disk: None,
                        quantization_config: None,
                    }
                )),
            }),
            ..Default::default()
        }).await.map_err(|e| AppError::QdrantError(e.to_string()))?;

        println!("Collection 'chat_memory' created");
    }

    Ok(())
}


pub async fn store_message_to_qdrant(
    client: &Qdrant,
    session_id: &str,
    role: &str,
    content: &str,
    embedding: Vec<f32>,
    timestamp: i64,
) -> AppResult<()> {
    let point = PointStruct::new(
        PointId::from(Uuid::new_v4().to_string()),
        Vectors::from(embedding),
        json!({
            "session_id": session_id,
            "role": role,
            "content": content,
            "timestamp": timestamp
        }).as_object().unwrap().clone(),
    );

    let upsert = UpsertPoints {
        collection_name: "chat_memory".to_string(),
        wait: Some(true),
        points: vec![point],
        ordering: None,
        shard_key_selector: None,
    };

    client.upsert_points(upsert)
        .await
        .map_err(|e| AppError::InternalError(format!("Qdrant upsert error: {}", e)))?;

    Ok(())
}

pub async fn search_context_from_qdrant(
    client: &Qdrant,
    session_id: &str,
    query_embedding: Vec<f32>,
) -> AppResult<Vec<String>> {
    let res = client.search_points(SearchPoints {
        collection_name: "chat_memory".to_string(),
        vector: query_embedding,
        limit: 5,
        filter: Some(Filter {
            must: vec![
                Condition::matches("session_id", session_id.to_string())
            ],
            ..Default::default()
        }),
        with_payload: Some(true.into()),
        ..Default::default()
    }).await.map_err(|e| AppError::QdrantError(e.to_string()))?;

    let history = res.result.into_iter()
        .filter_map(|point| {
            let role = point.payload.get("role")?.as_str()?;
            let content = point.payload.get("content")?.as_str()?;
            Some(format!("{}: {}", role, content))
        })
        .collect::<Vec<String>>();


    Ok(history)
}
