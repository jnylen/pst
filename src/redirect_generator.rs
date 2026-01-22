pub fn generate_redirect_html(target_url: &str) -> Vec<u8> {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="refresh" content="0; url={}">
    <title>Redirect</title>
    <script>window.location.href = "{}";</script>
</head>
<body>
    <p>Redirecting you to <a href="{}">{}</a></p>
</body>
</html>"#,
        target_url, target_url, target_url, target_url
    )
    .into_bytes()
}
