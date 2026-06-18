package utils

import (
	"encoding/json"
	"fmt"
	"math/rand"
)

func GenerateRandomLinkName(length int, namespace []rune) string {
	buffer := make([]rune, length)
	for i := range buffer {
		buffer[i] = namespace[rand.Intn(len(namespace))]
	}
	return string(buffer)
}

func BuildRootUrl(protocol string, domain string, port int, tls bool) string {
	var rootUrl string
	if protocol != "" {
		rootUrl = fmt.Sprint(protocol, "://", domain)
	} else {
		if tls {
			rootUrl = fmt.Sprintf("https://%s", domain)
		} else {
			rootUrl = fmt.Sprintf("http://%s", domain)
		}
	}

	if port != 80 {
		rootUrl += fmt.Sprintf(":%d", port)
	}

	rootUrl += "/"

	return rootUrl
}

func IsValidJson(s string) bool {
	var js map[string]interface{}
	return json.Unmarshal([]byte(s), &js) == nil
}
