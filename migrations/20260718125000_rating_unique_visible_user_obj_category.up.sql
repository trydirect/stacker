DELETE FROM rating a
USING rating b
WHERE a.id > b.id
  AND a.hidden = false
  AND b.hidden = false
  AND a.user_id = b.user_id
  AND a.obj_id = b.obj_id
  AND a.category = b.category;

CREATE UNIQUE INDEX IF NOT EXISTS rating_visible_user_obj_category_unique
ON rating (user_id, obj_id, category)
WHERE hidden = false;
