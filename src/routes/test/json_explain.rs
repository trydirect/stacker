use actix_web::{post, web, HttpResponse, Result, http::header::ContentType};

#[tracing::instrument(name = "Json explain")]
#[post("/json/explain/{line}/{column}")]
pub async fn handler(path: web::Path<(usize, usize,)>, body: web::Bytes) -> HttpResponse {
    let line = path.0;
    let column = path.1;

    let index = line_column_to_index(body.as_ref(), line, column);
    let body = String::from_utf8(body.as_ref()[..index].to_vec()).unwrap(); //todo unwrap

    HttpResponse::Ok()
        .content_type(ContentType::plaintext())
        .body(body)
}

fn line_column_to_index(u8slice: &[u8], line: usize, column: usize) -> usize {
    let mut l = 1;
    let mut c = 0;
    let mut i = 0;
    for ch in u8slice {
        i += 1;
        match ch {
            b'\n' => {
                l += 1;
                c = 0;
            }
            _ => {
                c += 1;
            }
        }
        if line == l && c == column {
            break;
        }
    }
    return i;
}
