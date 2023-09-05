# stacker


Run db migration
```
sqlx migrate run

```

Down migration

```
sqlx migrate revert 
```


Add rating 

```

 curl -vX POST 'http://localhost:8000/rating' -d '{"obj_id": 111, "category": "application", "comment":"some comment", "rate": 10}' --header 'Content-Type: application/json'

```