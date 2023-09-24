# Stacker


Stacker - is an application that helps users to create custom IT solutions based on dockerized open 
source apps and user's custom applications docker containers. Users can build their own stack of applications, and 
deploy the final result to their favorite clouds using TryDirect API.

Application development will include:
- Web UI (Application Stack builder)
- Command line interface
- Back-end RESTful API, includes:
  - [ ] Security module. 
    - [ ] User Authorization
  - [ ] Application Management
    - [ ] Application Key Management
  - [ ] Cloud Provider Key Management 
  - [ ] docker-compose.yml generator
  - [ ] TryDirect API Client
  - [ ] Rating module
   
## How to start 

#### Run db migration

```
sqlx migrate run

```


#### Down migration

```
sqlx migrate revert 
```


## CURL examples
#### Rate Product 

```

 curl -vX POST 'http://localhost:8000/rating' -d '{"obj_id": 1, "category": "application", "comment":"some comment", "rate": 10}' --header 'Content-Type: application/json'

```


#### Deploy 
```
curl -X POST -H "Content-Type: application/json" -d @custom-stack-payload-2.json http://127.0.0.1:8000/stack    
```