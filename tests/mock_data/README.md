This directory contains mock data for different endpoints

Here is some examples of making requests using CURL

Add new cloud credentials
curl -X POST -v  http://localhost:8000/cloud -d @cloud.json  --header 'Content-Type: application/json' -H "Authorization: Bearer $TD_BEARER"

Get cloud credentials
curl -X GET  http://localhost:8000/cloud/31  --header 'Content-Type: application/json' -H "Authorization: Bearer $TD_BEARER" 

