use crate::registry::state::ConduitState;

fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

/// Single-page local dashboard HTML (server-rendered).
pub fn render(state: &ConduitState, docker_ok: bool, proxy_status: Option<&str>) -> String {
    let mut projects_html = String::new();
    for (name, proj) in &state.projects {
        let mut routes = String::new();
        for (domain, target) in &proj.routes {
            routes.push_str(&format!(
                r#"<tr><td><a href="http://{}">{}</a></td><td>{}</td></tr>"#,
                esc(domain),
                esc(domain),
                esc(target)
            ));
        }
        if routes.is_empty() {
            routes = "<tr><td colspan=\"2\">(no HTTP routes)</td></tr>".into();
        }
        let mut services = String::new();
        for (svc, st) in &proj.services {
            let dom = st.domain.as_deref().map(esc).unwrap_or_else(|| "—".into());
            services.push_str(&format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                esc(svc),
                esc(&st.status),
                esc(&st.image),
                dom
            ));
        }
        if services.is_empty() {
            services = "<tr><td colspan=\"4\">(no services in state)</td></tr>".into();
        }
        projects_html.push_str(&format!(
            r##"<section class="card">
  <h3>{}</h3>
  <p class="meta">{} · network <code>{}</code> · compose <code>{}</code></p>
  <h4>Routes</h4>
  <table><thead><tr><th>URL</th><th>Target</th></tr></thead><tbody>{}</tbody></table>
  <h4>Services</h4>
  <table><thead><tr><th>Service</th><th>Status</th><th>Image</th><th>Domain</th></tr></thead><tbody>{}</tbody></table>
</section>"##,
            esc(name),
            esc(&proj.directory),
            esc(&proj.network),
            esc(&proj.compose_file),
            routes,
            services
        ));
    }

    if projects_html.is_empty() {
        projects_html =
            "<p class=\"empty\">No projects in state yet. Run <code>conduit up</code> in a compose project.</p>".into();
    }

    let docker_badge = if docker_ok {
        r#"<span class="ok">Docker OK</span>"#
    } else {
        r#"<span class="bad">Docker unreachable</span>"#
    };

    let proxy_line = match proxy_status {
        Some("running") => r#"<span class="ok">conduit-proxy: running</span>"#.to_string(),
        Some(s) => format!(r#"<span class="warn">conduit-proxy: {}</span>"#, esc(s)),
        None if docker_ok => "<span class=\"muted\">conduit-proxy: not created</span>".into(),
        None => "<span class=\"muted\">conduit-proxy: —</span>".into(),
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>Conduit</title>
  <link rel="preconnect" href="https://fonts.googleapis.com"/>
  <link href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,400;0,9..40,600;1,9..40,400&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet"/>
  <style>
    :root {{
      --bg: #0c0e12;
      --panel: #141820;
      --border: #252b36;
      --text: #e8eaef;
      --muted: #8b93a5;
      --accent: #3dff9a;
      --warn: #ffc14a;
      --bad: #ff6b6b;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0; min-height: 100vh;
      font-family: "DM Sans", system-ui, sans-serif;
      background: var(--bg);
      color: var(--text);
      background-image:
        radial-gradient(ellipse 80% 50% at 20% 0%, rgba(61,255,154,0.08), transparent 50%),
        radial-gradient(ellipse 60% 40% at 100% 100%, rgba(100,120,255,0.06), transparent 45%);
    }}
    header {{
      padding: 1.75rem 2rem;
      border-bottom: 1px solid var(--border);
      display: flex; flex-wrap: wrap; align-items: baseline; gap: 1rem;
    }}
    h1 {{
      margin: 0;
      font-size: 1.5rem;
      font-weight: 600;
      letter-spacing: -0.03em;
    }}
    .status {{ font-family: "JetBrains Mono", monospace; font-size: 0.8rem; display: flex; gap: 1rem; flex-wrap: wrap; }}
    .ok {{ color: var(--accent); }}
    .bad {{ color: var(--bad); }}
    .warn {{ color: var(--warn); }}
    .muted {{ color: var(--muted); }}
    main {{ padding: 2rem; max-width: 960px; margin: 0 auto; }}
    .card {{
      background: var(--panel);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 1.25rem 1.5rem;
      margin-bottom: 1.5rem;
    }}
    .card h3 {{ margin: 0 0 0.5rem; font-size: 1.15rem; }}
    .meta {{ color: var(--muted); font-size: 0.85rem; margin: 0 0 1rem; word-break: break-all; }}
    h4 {{ font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin: 1rem 0 0.5rem; }}
    table {{ width: 100%; border-collapse: collapse; font-family: "JetBrains Mono", monospace; font-size: 0.8rem; }}
    th, td {{ text-align: left; padding: 0.45rem 0.6rem; border-bottom: 1px solid var(--border); }}
    th {{ color: var(--muted); font-weight: 500; }}
    a {{ color: var(--accent); text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    code {{ background: rgba(0,0,0,0.35); padding: 0.15rem 0.4rem; border-radius: 4px; font-size: 0.85em; }}
    .cheat {{
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
      gap: 0.75rem;
    }}
    .cheat div {{
      background: rgba(0,0,0,0.25);
      padding: 0.75rem;
      border-radius: 8px;
      font-size: 0.85rem;
    }}
    .cheat strong {{ display: block; color: var(--muted); font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.08em; margin-bottom: 0.35rem; }}
    .empty {{ color: var(--muted); }}
    footer {{ padding: 1rem 2rem 2rem; text-align: center; color: var(--muted); font-size: 0.8rem; }}
  </style>
</head>
<body>
  <header>
    <h1>Conduit</h1>
    <div class="status">{docker} · {proxy}</div>
  </header>
  <main>
    <section class="card">
      <h3>Quick commands</h3>
      <div class="cheat">
        <div><strong>Up</strong><code>conduit up</code></div>
        <div><strong>Status</strong><code>conduit ps</code></div>
        <div><strong>Routes</strong><code>conduit route</code></div>
        <div><strong>DB tunnel</strong><code>conduit db &lt;service&gt;</code></div>
        <div><strong>Down</strong><code>conduit down</code></div>
        <div><strong>Doctor</strong><code>conduit doctor</code></div>
      </div>
    </section>
    <h2 style="font-size:1rem; margin:0 0 1rem; font-weight:600;">Projects</h2>
    {projects}
  </main>
  <footer>JSON: <a href="/api/state">/api/state</a> · Refresh this page after <code>conduit up</code></footer>
</body>
</html>"##,
        docker = docker_badge,
        proxy = proxy_line,
        projects = projects_html
    )
}
