<a href="https://discord.gg/mNhsa8VdYX"><img alt="Discord" src="https://img.shields.io/discord/578119430391988232?label=discord"></a>
# Stacker

<br><br><br><br>
<div align="center">
<img width="300" src="https://raw.githubusercontent.com/trydirect/stacker/assets/logo/stacker.png"> 
 </div>

Stacker - is an application that helps users to create custom IT solutions based on dockerized open 
source apps and user's custom applications docker containers. Users can build their own stack of applications, and 
deploy the final result to their favorite clouds using TryDirect API.

Application development will include:
- Web UI (Application Stack builder)
- Command line interface
- Back-end RESTful API, includes:
  - [ ] Security module. 
    - [ ] User Authorization
  - [ ] Restful API client Application Management
    - [ ] Application Key Management
  - [ ] Cloud Provider Key Management 
  - [ ] docker-compose.yml generator
  - [ ] TryDirect API Client
  - [ ] Rating module
   
## How to start 


## Structure

Stacker (User's dashboard) - Management 
----------
Authentication through TryDirect OAuth
/api/auth checks client's creds, api token, api secret
/apiclient (Create/Manage user's api clients) example: BeerMaster (email, callback)
/rating   


Stacker (API) - Serves API clients 
----------
Authentication made through TryDirect OAuth, here we have only client 
Database (Read only)
Logging/Tracing (Files) / Quickwit for future 
/stack (WebUI, as a result we have a JSON)
/stack/deploy -> sends deploy command to TryDirect Install service 
/stack/deploy/status - get installation progress (rabbitmq client),

#### TODO 
Find out how to get user's token for queue
Limit Requests Frequency (Related to user's settings/role/group etc)
Callback event, example: subscribe on get_report (internally from rabbitmq client),


main client (AllMasters) ->  client (Beermaster) 


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

#### Create API Client
```
curl -X POST http://localhost:8000/client  --header 'Content-Type: application/json' -H "Authorization: Bearer $TD_BEARER"
```

Test client's app deploy
```
http://localhost:8000/test/deploy
```
