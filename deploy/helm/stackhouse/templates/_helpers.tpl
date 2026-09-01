{{/*
Expand the name of the chart.
*/}}
{{- define "stackhouse.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "stackhouse.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "stackhouse.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "stackhouse.labels" -}}
helm.sh/chart: {{ include "stackhouse.chart" . }}
{{ include "stackhouse.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "stackhouse.selectorLabels" -}}
app.kubernetes.io/name: {{ include "stackhouse.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Service account name
*/}}
{{- define "stackhouse.serviceAccountName" -}}
{{- if .Values.stackhouse.serviceAccount.create }}
{{- default (include "stackhouse.fullname" .) .Values.stackhouse.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.stackhouse.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Qdrant internal URL
*/}}
{{- define "stackhouse.qdrantUrl" -}}
{{- printf "http://%s-qdrant:6333" .Release.Name }}
{{- end }}

{{/*
PostgreSQL internal URL
*/}}
{{- define "stackhouse.databaseUrl" -}}
{{- printf "postgres://%s:%s@%s-postgresql:5432/%s" .Values.postgresql.auth.username .Values.postgresql.auth.password .Release.Name .Values.postgresql.auth.database }}
{{- end }}

{{/*
Redis internal URL
*/}}
{{- define "stackhouse.redisUrl" -}}
{{- printf "redis://:%s@%s-redis-master:6379/0" .Values.redis.auth.password .Release.Name }}
{{- end }}
