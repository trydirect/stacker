DELETE FROM project_app
WHERE lower(replace(code, '-', '_')) = 'nginx_proxy_manager';
